//! The `typst::World` implementation shared by the PDF and HTML backends.
//!
//! Both backends compile the same single in-memory source with the same font
//! and file-resolution rules; the only thing that differs between them is
//! which in-development [`Features`] the library is built with (HTML export
//! requires `Feature::Html`, PDF requires none) and which document type they
//! ask `typst::compile` for.
//!
//! ── Packages are resolved the same way typst-cli does ───────────────────────
//! With the `network` feature (on by default), `@preview/...` imports —
//! Typst Universe templates and libraries — go through `typst-kit`'s
//! `SystemFiles`: local package data/cache directories are checked first,
//! then a download from packages.typst.org is cached under the system cache
//! dir (`~/.cache/typst/packages` and platform equivalents). Without the
//! feature, only project-relative paths resolve and a package import fails
//! with "not found" — the crate still emits and compiles plain `.typ`.

#[cfg(not(feature = "network"))]
use typst::diag::FileError;
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Features, Library, LibraryExt, World};
use typst_kit::fonts::FontStore;

#[cfg(feature = "network")]
use typst_kit::{
    downloader::SystemDownloader,
    files::{FileStore, FsRoot, SystemFiles},
    packages::SystemPackages,
};

pub(crate) struct ExportWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
    #[cfg(feature = "network")]
    files: FileStore<SystemFiles>,
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

        #[cfg(feature = "network")]
        let files = FileStore::new(SystemFiles::new(
            FsRoot::new(std::env::current_dir().unwrap_or_else(|_| ".".into())),
            SystemPackages::new(SystemDownloader::new(concat!(
                "twig-export/",
                env!("CARGO_PKG_VERSION")
            ))),
        ));

        ExportWorld {
            library: LazyHash::new(library),
            fonts,
            main: Source::new(id, typ.into()),
            #[cfg(feature = "network")]
            files,
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

    #[cfg(feature = "network")]
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            self.files.source(id)
        }
    }

    #[cfg(not(feature = "network"))]
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    /// Besides `#import`s of `@preview/...` packages (only resolved with the
    /// `network` feature), the only files Typst asks for are the images a
    /// document links to and any files a package itself reads. Both resolve
    /// against the working directory / the package's own root.
    #[cfg(feature = "network")]
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    #[cfg(not(feature = "network"))]
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
