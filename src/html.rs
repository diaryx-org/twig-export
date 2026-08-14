//! HTML output: compile the emitted Typst in-process, through Typst's
//! experimental HTML backend.
//!
//! ── Why this is opt-in and PDF is not ──────────────────────────────────────
//! Typst itself refuses to compile to HTML unless the `Feature::Html` flag is
//! set, and warns when it is: "html export is under active development and
//! incomplete... do not rely on this feature for production use cases." So
//! unlike `pdf`, this feature defaults off.
//!
//! It is not just an upstream disclaimer, either — testing it against this
//! crate's own output found that Typst's HTML converter has no case for
//! `align(..)[..]` or `pad(..)[..]` calls and silently drops their *content*
//! (not just the styling) rather than erroring. `typst_emit.rs` now emits
//! those as scoped `#set` rules instead, which HTML export does handle, but
//! any new PDF-oriented construct added there should be checked against this
//! backend before being trusted.
//!
//! See [`crate::typst_world`] for the shared `World` implementation.

use anyhow::{Result, anyhow, bail};
use typst::{Feature, Features, WorldExt};

use crate::typst_world::ExportWorld;

/// Compile Typst source to an HTML document.
pub fn compile(typ: &str) -> Result<String> {
    let world = ExportWorld::new(typ, Features::from_iter([Feature::Html]));
    let result = typst::compile::<typst_html::HtmlDocument>(&world);

    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            // Typst diagnostics point into generated source the user never
            // wrote, so quote the offending line as well as the message.
            let mut msg = String::from("typst failed to compile the generated document to HTML:\n");
            for e in errors.iter().take(10) {
                msg.push_str(&format!("  - {}\n", e.message));
                if let Some(range) = world.range(e.span)
                    && let Some(text) = world.main_text().get(range)
                {
                    msg.push_str(&format!("    at: {}\n", text.trim()));
                }
            }
            msg.push_str(
                "\nThis is a twig-export bug or a bad `raw.typst` escape hatch in your config.\n\
                 Re-run with `-o out.typ` to inspect the generated source.",
            );
            bail!(msg);
        }
    };

    typst_html::html(&document, &typst_html::HtmlOptions { pretty: true })
        .map_err(|e| anyhow!("HTML writing failed: {e:?}"))
}
