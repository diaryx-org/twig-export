//! PDF output: compile the emitted Typst in-process.
//!
//! ── Why link the compiler instead of shelling out ──────────────────────────
//! Linking means `twig-export doc.md -o out.pdf` works from a single binary
//! with no `typst` on `PATH` and no version skew between what we emit and what
//! compiles it. The cost is a large dependency tree and a pinned Typst
//! version, which is why this whole module sits behind the `pdf` feature: the
//! AST→Typst mapping — all of the logic worth testing — builds and tests
//! without it.
//!
//! The `World` this compiles against — font loading, file resolution, the
//! reproducible-clock stub — is shared with the `html` backend; see
//! [`crate::typst_world`].

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use typst::{Features, WorldExt};

use crate::typst_world::ExportWorld;

/// Compile Typst source to PDF bytes, its relative paths (pictures)
/// resolved against the working directory.
pub fn compile(typ: &str) -> Result<Vec<u8>> {
    write(&ExportWorld::new(typ, Features::default()))
}

/// Compile Typst source to PDF bytes, its relative paths resolved against
/// `root`: the folder of the document it was rendered from.
pub fn compile_in(typ: &str, root: &Path) -> Result<Vec<u8>> {
    write(&ExportWorld::rooted(typ, Features::default(), root))
}

fn write(world: &ExportWorld) -> Result<Vec<u8>> {
    let result = typst::compile::<typst_layout::PagedDocument>(world);

    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            // Typst diagnostics point into generated source the user never
            // wrote, so quote the offending line as well as the message.
            let mut msg = String::from("typst failed to compile the generated document:\n");
            for e in errors.iter().take(10) {
                msg.push_str(&format!("  - {}\n", e.message));
                // Quote the offending source. Typst is pointing into markup we
                // generated, not anything the user wrote, so the message alone
                // is rarely enough to act on.
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

    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|e| anyhow!("PDF writing failed: {e:?}"))
}
