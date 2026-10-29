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
pub mod templates;
pub mod typst_emit;

#[cfg(any(feature = "pdf", feature = "html"))]
mod typst_world;

#[cfg(feature = "pdf")]
pub mod pdf;

#[cfg(feature = "pdf")]
pub mod tiles;

#[cfg(feature = "install")]
pub mod install;

pub mod adapt;

#[cfg(feature = "html")]
pub mod html;

use anyhow::{Context, Result};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

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

/// What [`render_words`] produced: the Typst source with a hole for each run
/// of the author's text, the words that fill them, and where each top-level
/// block of the source starts. See [`typst_emit::emit_words`].
pub struct Words {
    pub typst: String,
    pub words: Vec<typst_emit::Word>,
    /// The byte each top-level block of the source starts at, in order: what
    /// a word's place is counted from, so that typing in one block leaves the
    /// places of the words in every other as they were.
    pub blocks: Vec<usize>,
    /// The files the source reads that are no file on disk: the pictures a
    /// host handed over ([`Hooks::picture`]) and those written into the
    /// document as `data:` URLs, each at its path in the source.
    pub files: Vec<(String, Arc<[u8]>)>,
    /// The bytes of the source each object was set from, by its number: a
    /// picture, or the room left under a block ([`Hooks::room`]). Set, each
    /// is one glyph, the object replacement character, as big as the
    /// object ([`pdf::Layout::pages`]).
    pub objects: Vec<Range<usize>>,
}

/// A picture as a host finds it ([`Hooks::picture`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picture {
    /// The destination as written, a path the typesetter reads against
    /// the document's folder.
    AsWritten,
    /// The picture's file, read.
    Bytes(Arc<[u8]>),
    /// There is none: its alt text is set in its place.
    Missing,
}

/// What a host answers while a document is rendered as words
/// ([`render_words_with`]), beyond the config: where its pictures are, and
/// room it draws in itself. The defaults ask for neither.
pub trait Hooks {
    /// The picture at `destination`, as the document writes it. A `data:`
    /// URL is never asked about: its picture is in it.
    fn picture(&self, destination: &str) -> Picture {
        let _ = destination;
        Picture::AsWritten
    }

    /// How tall a room to leave under the block whose source starts at
    /// byte `at`, in points, for the host to draw in: an object as wide as
    /// the block's column and this tall, which draws nothing.
    fn room(&self, at: usize) -> Option<f64> {
        let _ = at;
        None
    }
}

/// The [`Hooks`] that ask for nothing.
struct NoHooks;
impl Hooks for NoHooks {}

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
    let td = parse(source, format, extensions)?;
    render_parsed(td, source, cfg)
}

/// Apply `cfg` to `td`, a document already parsed from `source`, and emit
/// Typst. For a caller that parses with features of its own, as an editor
/// does, so the export reads the document the editor reads.
pub fn render_parsed(td: twig::Document, source: &str, cfg: &config::Config) -> Result<Rendered> {
    let (doc, cfg, resolved, unfired_bindings) = prepare(td, source, cfg)?;
    let typst = typst_emit::emit(&doc, &cfg, &resolved);
    Ok(Rendered {
        typst,
        match_counts: resolved.match_counts.clone(),
        unfired_bindings,
    })
}

/// As [`render_parsed`], with the author's words left out of the source as
/// holes and handed back beside it, for [`pdf::Typesetter::lay_out`] to set
/// as content.
pub fn render_words(td: twig::Document, source: &str, cfg: &config::Config) -> Result<Words> {
    render_words_with(td, source, cfg, &NoHooks)
}

/// As [`render_words`], asking `hooks` where the pictures are and what room
/// to leave.
pub fn render_words_with(
    td: twig::Document,
    source: &str,
    cfg: &config::Config,
    hooks: &dyn Hooks,
) -> Result<Words> {
    let (doc, cfg, resolved, _) = prepare(td, source, cfg)?;
    let typst_emit::Emitted {
        typst,
        words,
        files,
        objects,
    } = typst_emit::emit_words(&doc, &cfg, &resolved, hooks);
    let blocks = doc
        .node(doc.root())
        .children
        .iter()
        .map(|&c| doc.node(c).span.0)
        .collect();
    Ok(Words {
        typst,
        words,
        blocks,
        files,
        objects,
    })
}

/// The tree ingested, the config with the document's frontmatter applied,
/// the styles resolved, and the frontmatter bindings that found nothing.
fn prepare(
    mut td: twig::Document,
    source: &str,
    cfg: &config::Config,
) -> Result<(doc::Doc, config::Config, resolve::Resolved, Vec<String>)> {
    let doc = doc::Doc::ingest(&mut td, source)?;

    // The document's frontmatter can override document-level settings, but
    // only through bindings the config declared — see `meta`.
    let mut cfg = cfg.clone();
    let mut unfired_bindings = Vec::new();
    if let Some((lang, text)) = doc.metadata() {
        let meta = meta::parse_metadata(lang, text)?;
        unfired_bindings = meta::apply_metadata(&mut cfg.document, &meta);
        // The template is given only the keys the config lists, or every
        // key, for `*`.
        let keys = cfg.document.template_meta.clone().unwrap_or_default();
        cfg.document.meta = if keys.iter().any(|k| k == "*") {
            meta.clone()
        } else {
            keys.iter()
                .filter_map(|k| Some((k.clone(), meta.get(k)?.clone())))
                .collect()
        };
    }

    // A template named by the config or bound from the frontmatter must be
    // one there can be: the emitter would otherwise set the document
    // without it, and silently.
    if let Some(name) = cfg.document.template.as_deref() {
        templates::check(name)?;
    }

    let resolved = resolve::resolve(&cfg, &doc, &mut td)?;
    Ok((doc, cfg, resolved, unfired_bindings))
}
