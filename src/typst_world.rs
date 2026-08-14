//! The `typst::World` implementation shared by the PDF and HTML backends.
//!
//! Both backends compile the same single in-memory source with the same font
//! and file-resolution rules; the only thing that differs between them is
//! which in-development [`Features`] the library is built with (HTML export
//! requires `Feature::Html`, PDF requires none) and which document type they
//! ask `typst::compile` for.

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Features, Library, LibraryExt, World};
use typst_kit::fonts::FontStore;

pub(crate) struct ExportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
}

impl ExportWorld {
    pub(crate) fn new(typ: &str, features: Features) -> Self {
        // Embedded first, then system. Order is precedence, so a document
        // renders identically on a machine with no fonts installed and a
        // machine with a thousand — the embedded Libertinus / New Computer
        // Modern / DejaVu set always wins for the names it provides.
        let mut fonts = FontStore::new();
        fonts.extend(typst_kit::fonts::embedded());
        fonts.extend(typst_kit::fonts::system());

        let vpath = VirtualPath::new("main.typ").expect("main.typ is a valid virtual path");
        let id = RootedPath::new(VirtualRoot::Project, vpath).intern();

        let library = Library::builder().with_features(features).build();

        ExportWorld {
            library: LazyHash::new(library),
            fonts,
            main: Source::new(id, typ.into()),
        }
    }

    /// The generated source text, for quoting the offending line in a
    /// compile-error message.
    pub(crate) fn main_text(&self) -> &str {
        self.main.text()
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
