//! Render [Twig](https://github.com/diaryx-org/twig) document ASTs to Typst
//! and PDF, with a configurable map from in-text directives and document
//! metadata to styling.
//!
//! ── Why this is a separate crate from twig ─────────────────────────────────
//! Twig's charter says it supports "round-trippable" formats and explicitly
//! rules out first-class presentation formats like PDF (`twig.md`). But its
//! architecture already anticipates one: `src/format.zig` splits `Format`
//! (what twig can PARSE) from `Target` (what twig can WRITE) specifically so
//! that an *export-only* target — one nothing reads back, `reads_back_as =
//! null` — is expressible, and both `format.zig` and `DESIGN.md` name PDF as
//! the motivating case.
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

#[cfg(feature = "pdf")]
pub mod pdf;

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

/// Detect a twig input format from a file extension, mirroring the extensions
/// twig's own CLI infers (`twig --help`).
pub fn format_from_path(path: &Path) -> Option<twig::Format> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "dj" | "djot" => twig::Format::Djot,
            "md" | "markdown" => twig::Format::Markdown,
            "xml" => twig::Format::Xml,
            "html" | "htm" => twig::Format::Html,
            _ => return None,
        },
    )
}

/// Parse `source`, apply `cfg`, and emit Typst.
pub fn render(
    source: &str,
    format: twig::Format,
    cfg: &config::Config,
    extensions: twig::MarkdownExtensions,
) -> Result<Rendered> {
    let mut td = twig::Document::parse_str_with(source, format, extensions)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("twig failed to parse the document")?;

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
