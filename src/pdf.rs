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
//!
//! ── Setting one document again and again ───────────────────────────────────
//! [`compile`] and [`compile_in`] read the system's fonts on every call,
//! which is most of the time a short document takes. A caller that sets
//! documents repeatedly, as an editor's live preview does, keeps a
//! [`Typesetter`]: the fonts are read once, and Typst's own memoization
//! makes a compile after a small edit cheaper still. With the `svg` feature
//! it also sets each page as an SVG, for a preview to draw.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use typst::{Features, WorldExt};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::typst_world::{ExportWorld, fonts};

/// Compile Typst source to PDF bytes, its relative paths (pictures)
/// resolved against the working directory.
pub fn compile(typ: &str) -> Result<Vec<u8>> {
    write(&lay_out(&ExportWorld::new(typ, Features::default()))?)
}

/// Compile Typst source to PDF bytes, its relative paths resolved against
/// `root`: the folder of the document it was rendered from.
pub fn compile_in(typ: &str, root: &Path) -> Result<Vec<u8>> {
    write(&lay_out(&ExportWorld::rooted(
        typ,
        Features::default(),
        root,
    ))?)
}

/// What sets Typst source again and again: the fonts, read once when it is
/// made. Cheap to clone, and shared between threads.
#[derive(Clone)]
pub struct Typesetter {
    fonts: Arc<FontStore>,
}

impl Typesetter {
    /// A typesetter with the embedded fonts and the system's, read now.
    pub fn new() -> Self {
        Typesetter {
            fonts: Arc::new(fonts()),
        }
    }

    /// Typst source as PDF bytes, its relative paths resolved against
    /// `root`, as [`compile_in`] sets it.
    pub fn pdf(&self, typ: &str, root: &Path) -> Result<Vec<u8>> {
        write(&self.lay_out(typ, root)?)
    }

    /// Typst source as one SVG document per page, its relative paths
    /// resolved against `root`. Each page is the size the PDF's is, in
    /// points, and draws its text as outlines, so it looks the same
    /// wherever it is drawn.
    #[cfg(feature = "svg")]
    pub fn svg(&self, typ: &str, root: &Path) -> Result<Vec<String>> {
        let document = self.lay_out(typ, root)?;
        let opts = typst_svg::SvgOptions::default();
        Ok(document
            .pages()
            .iter()
            .map(|page| typst_svg::svg(page, &opts))
            .collect())
    }

    fn lay_out(&self, typ: &str, root: &Path) -> Result<PagedDocument> {
        lay_out(&ExportWorld::with_fonts(
            typ,
            Features::default(),
            root,
            Arc::clone(&self.fonts),
        ))
    }
}

impl Default for Typesetter {
    fn default() -> Self {
        Self::new()
    }
}

fn lay_out(world: &ExportWorld) -> Result<PagedDocument> {
    let result = typst::compile::<PagedDocument>(world);

    match result.output {
        Ok(doc) => Ok(doc),
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
    }
}

fn write(document: &PagedDocument) -> Result<Vec<u8>> {
    typst_pdf::pdf(document, &typst_pdf::PdfOptions::default())
        .map_err(|e| anyhow!("PDF writing failed: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "#set page(width: 200pt, height: 100pt)\n= One\n#pagebreak()\n= Two";

    #[test]
    fn a_typesetter_sets_what_compile_in_sets() {
        let ts = Typesetter::new();
        let root = Path::new(".");
        let pdf = ts.pdf(DOC, root).expect("a pdf");
        assert!(pdf.starts_with(b"%PDF"));
        // Again, through the same fonts: the second compile is the first's.
        assert_eq!(ts.pdf(DOC, root).unwrap(), compile_in(DOC, root).unwrap());
    }

    #[test]
    fn a_typesetter_says_why_it_cannot_set() {
        let err = Typesetter::new()
            .pdf("#nonesuch()", Path::new("."))
            .unwrap_err();
        assert!(err.to_string().contains("nonesuch"), "{err:#}");
    }

    #[cfg(feature = "svg")]
    #[test]
    fn each_page_is_an_svg_the_size_of_the_pdfs() {
        let pages = Typesetter::new().svg(DOC, Path::new(".")).expect("pages");
        assert_eq!(pages.len(), 2);
        for page in &pages {
            assert!(page.starts_with("<svg"), "{}", &page[..40.min(page.len())]);
            assert!(page.contains(r#"viewBox="0 0 200 100""#), "{page}");
        }
    }
}
