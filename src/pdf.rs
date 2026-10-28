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
//!
//! ── Pages an editor can draw on ────────────────────────────────────────────
//! [`Typesetter::lay_out`] sets a document from [`crate::render_words`]: the
//! source with holes, and the author's words, each made Typst content here
//! with a span into the document's own source. The [`Layout`] it answers is
//! one compile, as a PDF, as SVG pages, and as each page's [`Glyph`]s, every
//! one with the bytes of the source it was set from: what an editor needs to
//! put a caret on the page, or a click on the page into the text.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use typst::comemo::Track;
use typst::diag::{At, SourceResult, Warned};
use typst::foundations::{Array, Context, Func, NativeElement, Value};
use typst::layout::{Frame, FrameItem, Point, Transform};
use typst::syntax::{FileId, RootedPath, Span, SpanKind, VirtualPath, VirtualRoot};
use typst::text::{RawContent, RawElem, TextElem};
use typst::{Features, World, WorldExt};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

use crate::typst_emit::{TEMPLATE, Word};
pub use crate::typst_world::Packages;
use crate::typst_world::{ExportWorld, fonts};

/// Compile Typst source to PDF bytes, its relative paths (pictures)
/// resolved against the working directory.
pub fn compile(typ: &str) -> Result<Vec<u8>> {
    let world = ExportWorld::new(typ, Features::default());
    write(&checked(&world, typst::compile(&world))?)
}

/// Compile Typst source to PDF bytes, its relative paths resolved against
/// `root`: the folder of the document it was rendered from.
pub fn compile_in(typ: &str, root: &Path) -> Result<Vec<u8>> {
    let world = ExportWorld::rooted(typ, Features::default(), root);
    write(&checked(&world, typst::compile(&world))?)
}

/// What sets Typst source again and again: the fonts, read once when it is
/// made, and where it finds packages. Cheap to clone, and shared between
/// threads.
#[derive(Clone)]
pub struct Typesetter {
    fonts: Arc<FontStore>,
    packages: Packages,
}

impl Typesetter {
    /// A typesetter with the embedded fonts and the system's, read now, and
    /// the built-in templates.
    pub fn new() -> Self {
        Typesetter {
            fonts: Arc::new(fonts()),
            packages: Packages::default(),
        }
    }

    /// This typesetter, finding packages in `packages`.
    pub fn with_packages(mut self, packages: Packages) -> Self {
        self.packages = packages;
        self
    }

    /// Typst source as PDF bytes, its relative paths resolved against
    /// `root`, as [`compile_in`] sets it.
    pub fn pdf(&self, typ: &str, root: &Path) -> Result<Vec<u8>> {
        write(&self.set(typ, root)?)
    }

    /// Typst source as one SVG document per page, its relative paths
    /// resolved against `root`. Each page is the size the PDF's is, in
    /// points, and draws its text as outlines, so it looks the same
    /// wherever it is drawn.
    #[cfg(feature = "svg")]
    pub fn svg(&self, typ: &str, root: &Path) -> Result<Vec<String>> {
        let document = self.set(typ, root)?;
        let opts = typst_svg::SvgOptions::default();
        Ok(document
            .pages()
            .iter()
            .map(|page| typst_svg::svg(page, &opts))
            .collect())
    }

    /// A document rendered by [`crate::render_words`], its relative paths
    /// resolved against `root`, laid out: the pages [`Self::pdf`] sets from
    /// [`crate::render`]'s source, with every glyph's place in the source
    /// the document was rendered from.
    pub fn lay_out(&self, words: &crate::Words, root: &Path) -> Result<Layout> {
        let world = self.world(&words.typst, root);
        let file = source_file();
        let values: Array = words
            .words
            .iter()
            .map(|w| Value::Content(content(w, &words.blocks, file)))
            .collect();
        let value = Value::Array(values);
        let set = typst::compile_with::<PagedDocument>(&world, &|engine, module| {
            let template = module
                .scope()
                .get(TEMPLATE)
                .map(|b| b.read().clone())
                .ok_or("the source defines no template")
                .at(Span::detached())?
                .cast::<Func>()
                .at(Span::detached())?;
            Ok(template
                .call(engine, Context::none().track(), [value.clone()])?
                .display())
        });
        Ok(Layout {
            document: checked(&world, set)?,
            blocks: words.blocks.clone(),
            file,
        })
    }

    fn set(&self, typ: &str, root: &Path) -> Result<PagedDocument> {
        let world = self.world(typ, root);
        checked(&world, typst::compile(&world))
    }

    fn world(&self, typ: &str, root: &Path) -> ExportWorld {
        ExportWorld::with_fonts(typ, Features::default(), root, Arc::clone(&self.fonts))
            .with_packages(self.packages.clone())
    }
}

/// The file a word's span names: the document's own source, which Typst
/// never reads. A span into it is a raw range used as a pair: the start is
/// the top-level block the word is in, the end its offset from the block's
/// start. Counted from the block, a word's span stays the same while
/// another block is edited, and a span is part of what Typst's cache keys
/// an element's layout by.
fn source_file() -> FileId {
    let vpath = VirtualPath::new("twig-source").expect("a valid virtual path");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

/// A word as Typst content, spanned to its place in the source: text, or
/// code with a span for each line.
fn content(w: &Word, blocks: &[usize], file: FileId) -> typst::foundations::Content {
    let span = |at: usize| {
        let block = blocks.partition_point(|&b| b <= at).saturating_sub(1);
        let from = blocks.get(block).copied().unwrap_or(0);
        Span::from_range(file, block..at - from)
    };
    let Some(code) = &w.code else {
        return TextElem::packed(w.text.as_str()).spanned(span(w.at));
    };
    let lines = w
        .text
        .split('\n')
        .zip(&code.lines)
        .map(|(line, &at)| (line.into(), span(at)))
        .collect();
    RawElem::new(RawContent::Lines(lines))
        .with_block(code.block)
        .with_lang(code.lang.as_deref().map(Into::into))
        .pack()
        .spanned(span(w.at))
}

/// One compile of a document: its pages, and where each glyph came from.
pub struct Layout {
    document: PagedDocument,
    blocks: Vec<usize>,
    file: FileId,
}

/// A laid-out page's size and glyphs, in points from its top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
}

/// A glyph as laid out: where it is on its page, in points from the page's
/// top left, and the bytes of the source it was set from. `source` is
/// `None` for what Typst draws itself (a bullet, a footnote's number, a
/// page number) or a template's own text.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The left of the glyph's advance.
    pub x: f64,
    pub baseline: f64,
    /// The glyph's advance.
    pub width: f64,
    /// The face's ascender and descender at this size, above and below the
    /// baseline: the height a caret beside the glyph is drawn.
    pub ascent: f64,
    pub descent: f64,
    /// The text the glyph draws: one character, or several for a ligature.
    pub text: String,
    pub source: Option<Range<usize>>,
}

impl Layout {
    /// The pages as PDF bytes.
    pub fn pdf(&self) -> Result<Vec<u8>> {
        write(&self.document)
    }

    /// Each page as an SVG document, as [`Typesetter::svg`] sets it.
    #[cfg(feature = "svg")]
    pub fn svg(&self) -> Vec<String> {
        let opts = typst_svg::SvgOptions::default();
        self.document
            .pages()
            .iter()
            .map(|page| typst_svg::svg(page, &opts))
            .collect()
    }

    /// Each page's size and glyphs.
    pub fn pages(&self) -> Vec<Page> {
        self.document
            .pages()
            .iter()
            .map(|page| {
                let mut glyphs = Vec::new();
                self.glyphs(&page.frame, Transform::identity(), &mut glyphs);
                Page {
                    width: page.frame.width().to_pt(),
                    height: page.frame.height().to_pt(),
                    glyphs,
                }
            })
            .collect()
    }

    fn glyphs(&self, frame: &Frame, ts: Transform, out: &mut Vec<Glyph>) {
        for (pos, item) in frame.items() {
            match item {
                FrameItem::Group(group) => {
                    let ts = ts
                        .pre_concat(Transform::translate(pos.x, pos.y))
                        .pre_concat(group.transform);
                    self.glyphs(&group.frame, ts, out);
                }
                FrameItem::Text(text) => {
                    let size = text.size;
                    let metrics = text.font.metrics();
                    let scale = ts.sy.get().abs();
                    let mut cursor = pos.x;
                    for g in &text.glyphs {
                        let at =
                            Point::new(cursor + g.x_offset.at(size), pos.y - g.y_offset.at(size))
                                .transform(ts);
                        let width = g.x_advance.at(size).to_pt() * ts.sx.get();
                        cursor += g.x_advance.at(size);
                        let drawn = &text.text[g.range()];
                        out.push(Glyph {
                            x: at.x.to_pt(),
                            baseline: at.y.to_pt(),
                            width,
                            ascent: metrics.ascender.at(size).to_pt() * scale,
                            descent: -metrics.descender.at(size).to_pt() * scale,
                            text: drawn.to_string(),
                            source: self.source(g.span, drawn.len()),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// The bytes of the source a glyph `len` bytes long came from, from its
    /// span and its offset into the text that span names.
    fn source(&self, (span, offset): (Span, u16), len: usize) -> Option<Range<usize>> {
        match span.get() {
            SpanKind::Range { id, range } if id == self.file => {
                let start = self.blocks.get(range.start)? + range.end + offset as usize;
                Some(start..start + len)
            }
            _ => None,
        }
    }
}

impl Default for Typesetter {
    fn default() -> Self {
        Self::new()
    }
}

/// What a compile set, or why it could not.
fn checked(
    world: &ExportWorld,
    result: Warned<SourceResult<PagedDocument>>,
) -> Result<PagedDocument> {
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
                if e.span.id() == Some(world.main())
                    && let Some(range) = world.range(e.span)
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
