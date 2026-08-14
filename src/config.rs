//! The meta-config: which directives and which metadata keys drive which
//! styling.
//!
//! ── Selectors are twig's, not ours ─────────────────────────────────────────
//! A rule's `select` is a twig CSS-lite selector (`src/ast/select.zig`),
//! resolved by twig's own engine. So anything you can find with
//! `twig query 'container[name=note][class~=warn]'` is something you can
//! style, with identical semantics, and there is no second grammar to learn or
//! to drift. See [`crate::doc`] for how ids are threaded across.
//!
//! ── Precedence is source order, not specificity ────────────────────────────
//! Rules apply top to bottom; later rules overlay earlier ones property by
//! property ([`Style::overlay`]). There is deliberately no CSS-style
//! specificity calculation.
//!
//! Specificity is the single most confusing thing about CSS, and its payoff —
//! order-independent stylesheets you can concatenate — is a payoff for
//! *distributed* authorship of huge sheets. A document style config is one
//! file, usually under a hundred lines, written by one person. Source order is
//! something you can see; specificity is something you have to compute. So the
//! rule is: put the general rule first, the specific one after.
//!
//! ── Two ways a document talks back ─────────────────────────────────────────
//! The config decides what the document is *allowed* to say about itself:
//!
//! - [`Document::from_metadata`] binds frontmatter keys to document settings,
//!   so `pdf-paper: a5` in a document's YAML only means something because the
//!   config said `paper = "pdf-paper"`. An unbound key is inert.
//! - Rules match on `[class~=…]` / `[key=…]`, so an in-text
//!   `{.warn}` / `::note` selects styling the config defined.
//!
//! In both directions the config is the authority and the document is the
//! input. A document cannot introduce styling the config did not sanction,
//! which is what makes it safe to render someone else's document.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::schema::{self, Finding};
use crate::style::{Style, Weight};

/// A whole config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct Config {
    pub document: Document,
    /// Style rules, applied in source order.
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
}

/// Page setup and document-wide defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct Document {
    /// A named paper size (`"a4"`, `"us-letter"`) — Typst's paper names.
    pub paper: Option<String>,
    pub margin: Option<String>,
    pub font: Option<String>,
    pub size: Option<String>,
    pub leading: Option<String>,
    pub align: Option<crate::style::Align>,
    /// Base language, for hyphenation and quote marks (`"en"`, `"de"`).
    pub lang: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    /// Page-number numbering pattern, e.g. `"1"` or `"— 1 —"`. `None` for no
    /// page numbers.
    pub page_numbering: Option<String>,
    /// Generate a table of contents at the top, to the given heading depth.
    pub toc_depth: Option<u8>,
    /// Typst emitted verbatim at the top of the preamble, after everything
    /// above. The document-level twin of [`crate::style::RawTypst`].
    pub raw_preamble: Option<String>,
    /// Typst emitted verbatim right after the preamble, before the body —
    /// where the title/author block goes. The default reproduces the classic
    /// centred title and author (reading them back via `context
    /// document.title`/`.author`, which is why a static default can still
    /// show per-document values: see `Default for Document`).
    ///
    /// This is deliberately just another raw-Typst field rather than a
    /// hardcoded Rust behaviour keyed off `title`/`author` being `Some`. A
    /// Typst Universe template that draws its own title page is not a
    /// special case to suppress — it is what you get by setting
    /// `title-page = ""` (or writing your own) like any other override.
    pub title_page: Option<String>,

    /// Bind frontmatter keys to the fields above: `paper = "pdf-paper"` means
    /// "if the document's metadata has a `pdf-paper` key, it overrides
    /// `document.paper`".
    ///
    /// The indirection is the point. Documents in the wild already carry
    /// `title`/`author`; a rendering-specific key like `pdf-paper` is one this
    /// config invents. Naming the binding explicitly means neither side has to
    /// guess the other's vocabulary, and a document key that nothing binds is
    /// inert rather than accidentally meaningful.
    pub from_metadata: BTreeMap<String, String>,
}

/// The stock title/author block. `context` reads `document.title`/`.author`
/// back from the `#set document(...)` this crate already emits, so this
/// static string still shows per-document values — see [`Document::title_page`].
// `document.title` reads back as `str` or `none`, but `document.author`
// ALWAYS reads back as an array — even after `#set document(author: "solo
// name")` — and defaults to `()`, not `none`, when never set. Hence `.len() >
// 0` rather than `!= none`, and `.join(", ")` for the (usual) single-author
// case as much as the multi-author one.
const DEFAULT_TITLE_PAGE: &str = "\
#context if document.title != none [
  #[
  #set align(center)
  #text(size: 1.8em, weight: \"bold\")[#document.title]
  ]
  #if document.author.len() > 0 [
    #[
    #set align(center)
    #text(size: 1.1em)[#document.author.join(\", \")]
    ]
  ]
]";

impl Default for Document {
    fn default() -> Self {
        Document {
            paper: Some("a4".into()),
            margin: Some("2.5cm".into()),
            font: None,
            size: Some("11pt".into()),
            leading: None,
            align: None,
            lang: Some("en".into()),
            title: None,
            author: None,
            page_numbering: None,
            toc_depth: None,
            raw_preamble: None,
            title_page: Some(DEFAULT_TITLE_PAGE.into()),
            from_metadata: BTreeMap::new(),
        }
    }
}

/// One selector → style binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Rule {
    /// A twig selector. Resolved by twig's own engine against the document.
    pub select: String,
    #[serde(default, flatten)]
    pub style: Style,
}

impl Config {
    /// Load a config from any language fig speaks — `.toml`, `.yaml`, `.json`,
    /// `.fig` — detected from the extension.
    ///
    /// Using fig here rather than a single hardcoded format is deliberate:
    /// the same parser reads the document's frontmatter (see [`crate::meta`]),
    /// whose language is whatever the author's fence tag said. One dependency
    /// covers both halves of "configurable", and a user who writes their notes
    /// in TOML frontmatter can write their style config in TOML too.
    ///
    /// Returns the config plus any non-fatal schema warnings. Rejections are
    /// errors; warnings are the caller's to surface (the CLI prints them).
    pub fn load(path: &Path) -> Result<(Self, Vec<Finding>)> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let lang = path.extension().and_then(|e| e.to_str()).unwrap_or("toml");
        Self::parse(&text, lang).with_context(|| format!("in config {}", path.display()))
    }

    /// Parse config text written in the fig language named by `lang`.
    ///
    /// Validation runs against the `fig::Value` tree BEFORE deserialization,
    /// so a bad value is reported with its path and a near-miss suggestion
    /// rather than as a serde type error. See [`crate::schema`].
    pub fn parse(text: &str, lang: &str) -> Result<(Self, Vec<Finding>)> {
        let format = crate::meta::fig_format(lang)
            .with_context(|| format!("unknown config language {lang:?}"))?;

        let value = fig::Document::parse(text.as_bytes(), format)
            .and_then(|d| d.to_value())
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("config is not valid")?;

        let findings = schema::check(&value);
        schema::reject_on_errors(&findings)?;

        // The document is parsed twice — once structurally for the schema,
        // once through serde for the typed config. A style config is a
        // hundred lines; the clarity is worth more than the microseconds, and
        // fig has no `from_value`.
        let cfg: Config = fig::from_slice(text.as_bytes(), format)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("config does not match the schema")?;

        Ok((cfg, findings))
    }
}

/// Typst's spelling of a [`Weight`].
pub fn weight_str(w: Weight) -> &'static str {
    match w {
        Weight::Thin => "thin",
        Weight::Light => "light",
        Weight::Regular => "regular",
        Weight::Medium => "medium",
        Weight::Semibold => "semibold",
        Weight::Bold => "bold",
        Weight::Black => "black",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema runs before deserialization, so a bad value is reported with
    /// its path rather than as a serde type error.
    #[test]
    fn a_bad_value_fails_the_load_with_its_path() {
        let err = Config::parse("[[rule]]\nselect = \"heading\"\nsize = \"20\"\n", "toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("rule[0].size"), "must name the path: {err}");
        assert!(err.contains("has no unit"), "{err}");
    }

    /// A warning must NOT block the render — that is the whole point of the
    /// open/closed vocabulary split.
    #[test]
    fn a_warning_loads_successfully_and_is_reported() {
        let (cfg, warnings) = Config::parse("[document]\npaper = \"a9\"\n", "toml").unwrap();
        assert_eq!(cfg.document.paper.as_deref(), Some("a9"), "value passes through");
        assert_eq!(warnings.len(), 1);
        assert!(!warnings[0].validation.is_reject());
    }

    /// Every language fig speaks must work, since the same parser reads
    /// frontmatter whose language the author chose.
    #[test]
    fn a_config_can_be_written_in_any_fig_language() {
        let (toml, _) = Config::parse("[document]\npaper = \"a5\"\n", "toml").unwrap();
        let (yaml, _) = Config::parse("document:\n  paper: a5\n", "yaml").unwrap();
        let (json, _) = Config::parse(r#"{"document":{"paper":"a5"}}"#, "json").unwrap();

        assert_eq!(toml.document.paper.as_deref(), Some("a5"));
        assert_eq!(yaml.document.paper, toml.document.paper);
        assert_eq!(json.document.paper, toml.document.paper);
    }

    #[test]
    fn rules_keep_their_source_order() {
        let (cfg, _) = Config::parse(
            "[[rule]]\nselect = \"a\"\n\n[[rule]]\nselect = \"b\"\n",
            "toml",
        )
        .unwrap();
        let order: Vec<&str> = cfg.rules.iter().map(|r| r.select.as_str()).collect();
        assert_eq!(order, ["a", "b"], "precedence is source order");
    }
}
