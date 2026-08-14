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
//! ── The `World` is the whole interface ─────────────────────────────────────
//! Typst asks its embedder for four things: the standard library, a font book,
//! source files, and the current date. We have exactly one source file (the
//! string we just generated) and no package imports, so everything except
//! fonts is trivial. Fonts come from `typst-kit`, which searches the system
//! and falls back to the embedded Libertinus/DejaVu/New Computer Modern set —
//! so output is reproducible on a machine with no fonts installed.

use anyhow::{Result, anyhow, bail};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World, WorldExt};
use typst_kit::fonts::FontStore;

struct ExportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
}

impl ExportWorld {
    fn new(typ: &str) -> Self {
        // Embedded first, then system. Order is precedence, so a document
        // renders identically on a machine with no fonts installed and a
        // machine with a thousand — the embedded Libertinus / New Computer
        // Modern / DejaVu set always wins for the names it provides.
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());

        let vpath = VirtualPath::new("main.typ").expect("main.typ is a valid virtual path");
        let id = RootedPath::new(VirtualRoot::Project, vpath).intern();

        ExportWorld {
            library: LazyHash::new(Library::default()),
            fonts,
            main: Source::new(id, typ.into()),
        }
    }
}

impl World for ExportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    /// We emit a self-contained document with no `#include`, so the only files
    /// Typst asks for are the images a document links to. Resolve those
    /// against the working directory.
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = std::path::Path::new(id.vpath().get_without_slash());
        std::fs::read(path)
            .map(Bytes::new)
            .map_err(|e| FileError::from_io(e, path))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    /// `None` means "no reproducible clock", which makes `#datetime.today()`
    /// an error rather than silently baking today's date into a document that
    /// is supposed to render identically tomorrow.
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compile Typst source to PDF bytes.
pub fn compile(typ: &str) -> Result<Vec<u8>> {
    let world = ExportWorld::new(typ);
    let result = typst::compile::<typst_layout::PagedDocument>(&world);

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
                    && let Some(text) = world.main.text().get(range)
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
