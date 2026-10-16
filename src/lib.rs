//! Render [Twig](https://github.com/diaryx-org/twig) document ASTs to Typst
//! and PDF, with a configurable map from in-text directives and document
//! metadata to styling.
//!
//! ── Why this is a separate crate from twig ─────────────────────────────────
//! Twig's charter says it supports "round-trippable" formats and explicitly
//! rules out first-class presentation formats like PDF (`twig.md`). But its
//! architecture already anticipates one: `crates/twig/src/format.rs` splits
//! `Format` (what twig can PARSE) from `Target` (what twig can WRITE)
//! specifically so that an *export-only* target — one nothing reads back,
//! `reads_back_as = null` — is expressible, and twig's `docs/DESIGN.md` names
//! PDF as the motivating case.
//!
//! So this crate is that target, built outside twig's tree where the
//! presentation concerns live. It consumes twig's published AST and adds
//! nothing to twig's parse surface.
//!
//! ── The pipeline ──────────────────────────────────────────────────────────
//! ```text
//!   source ──twig──> AST ──[doc]──> owned tree (payloads + NodeIds)
//!                                        │
//!   config ──[config]──> rules ──────────┤
//!   frontmatter ──[meta]──> settings ────┤
//!                                        ▼
//!                                    [resolve] ──> style per node
//!                                        │
//!                                        ▼
//!                              [typst_emit] ──> .typ ──[pdf]──> PDF
//! ```
//!
//! The `.typ` stage is the real output and the PDF stage is a compile step on
//! top of it — see [`typst_emit`] for why that split is load-bearing rather
//! than incidental.

pub mod config;
pub mod doc;
pub mod meta;
pub mod resolve;
pub mod schema;
pub mod style;
pub mod typst_emit;

#[cfg(any(feature = "pdf", feature = "html"))]
mod typst_world;

#[cfg(feature = "pdf")]
pub mod pdf;

#[cfg(feature = "html")]
pub mod html;

use anyhow::{Context, Result};
use std::path::Path;

/// What a render produced, plus what the config did — the diagnostics are
/// here rather than printed inline so the library stays quiet and the CLI
/// decides what to show.
pub struct Rendered {
    pub typst: String,
    /// Per rule, in config order: how many nodes it matched. A zero is the
    /// usual explanation for "my styling did nothing".
    pub match_counts: Vec<usize>,
    /// Metadata bindings whose key was absent from the document.
    pub unfired_bindings: Vec<String>,
}

/// Opt-in syntax to parse with, named as twig names its Markdown features.
///
/// twig 6 has no `MarkdownExtensions`: a language declares named features and
/// a parse lays the ones asked for over its row. A format that does not
/// declare one of these ignores it, as twig 3 ignored Markdown's extensions
/// for every other format — see [`Extensions::features_for`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    /// Generic directives (`:name`, `::name`, `:::name`).
    pub directives: bool,
    /// `$…$` / `$$…$$` math.
    pub math: bool,
    /// Recognised raw HTML parsed into semantic nodes.
    pub html_elements: bool,
}

impl Extensions {
    /// The twig feature names turned on here that `format` declares.
    pub fn features_for(self, format: twig::Format) -> Vec<&'static str> {
        let declared = twig::format::features(format);
        [
            ("directives", self.directives),
            ("math", self.math),
            ("html_elements", self.html_elements),
        ]
        .into_iter()
        .filter(|&(name, on)| on && declared.iter().any(|f| f.name == name))
        .map(|(name, _)| name)
        .collect()
    }
}

/// The twig format a name or file extension names, mirroring the names
/// twig's own CLI accepts (`twig --help`). Each is a language crate of its
/// own, registered the first time it is asked for.
pub fn format_named(name: &str) -> Option<twig::Format> {
    Some(match name.to_ascii_lowercase().as_str() {
        "dj" | "djot" => twig_djot::format(),
        "md" | "markdown" => twig_markdown::format(),
        "xml" => twig_xml::format(),
        "html" | "htm" => twig_html::format(),
        _ => return None,
    })
}

/// Detect a twig input format from a file extension, mirroring the extensions
/// twig's own CLI infers (`twig --help`).
pub fn format_from_path(path: &Path) -> Option<twig::Format> {
    format_named(path.extension()?.to_str()?)
}

/// Parse `source` as `format` with the `extensions` it declares.
pub fn parse(source: &str, format: twig::Format, extensions: Extensions) -> Result<twig::Document> {
    twig::Document::parse_with_features(source.as_bytes(), format, &extensions.features_for(format))
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("twig failed to parse the document")
}

/// Parse `source`, apply `cfg`, and emit Typst.
pub fn render(
    source: &str,
    format: twig::Format,
    cfg: &config::Config,
    extensions: Extensions,
) -> Result<Rendered> {
    let mut td = parse(source, format, extensions)?;

    let doc = doc::Doc::ingest(&mut td, source)?;

    // The document's frontmatter can override document-level settings, but
    // only through bindings the config declared — see `meta`.
    let mut cfg = cfg.clone();
    let mut unfired_bindings = Vec::new();
    if let Some((lang, text)) = doc.metadata() {
        let meta = meta::parse_metadata(lang, text)?;
        unfired_bindings = meta::apply_metadata(&mut cfg.document, &meta);
    }

    let resolved = resolve::resolve(&cfg, &doc, &mut td)?;
    let typst = typst_emit::emit(&doc, &cfg, &resolved);

    Ok(Rendered {
        typst,
        match_counts: resolved.match_counts.clone(),
        unfired_bindings,
    })
}
