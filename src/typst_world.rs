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

/// The two things diaryx-org's Typst fork adds, used the way the content
/// route will: a template evaluated as Typst, called by `compile_with` with a
/// body built in Rust, whose text carries a raw-range span into a Markdown
/// file Typst never reads. If a rebase of the fork drops either patch, this
/// stops compiling; if `compile_with` stops laying out the body, or a glyph
/// stops carrying its span, it fails.
#[cfg(all(test, feature = "pdf"))]
mod fork_tests {
    use super::*;
    use typst::comemo::Track;
    use typst::foundations::{Context, Func, Value};
    use typst::layout::{Frame, FrameItem};
    use typst::syntax::{Span, SpanKind};
    use typst::text::TextElem;
    use typst_layout::PagedDocument;

    const TEMPLATE: &str = "#let book(body) = { set page(width: 10cm, height: auto); body }";

    fn spans(frame: &Frame, out: &mut Vec<(Span, String)>) {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Group(g) => spans(&g.frame, out),
                FrameItem::Text(text) => {
                    for g in &text.glyphs {
                        out.push((g.span.0, text.text[g.range()].to_string()));
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn compile_with_lays_out_a_body_with_spans_into_markdown() {
        let world = ExportWorld::new(TEMPLATE, Features::default());
        let md =
            RootedPath::new(VirtualRoot::Project, VirtualPath::new("doc.md").unwrap()).intern();
        // "Hello" at bytes 2..7 of `# Hello`.
        let body = TextElem::packed("Hello").spanned(Span::from_range(md, 2..7));

        let result = typst::compile_with::<PagedDocument>(&world, &|engine, module| {
            let book: Func = module
                .scope()
                .get("book")
                .expect("the template defines `book`")
                .read()
                .clone()
                .cast()
                .expect("`book` is a function");
            Ok(book
                .call(
                    engine,
                    Context::none().track(),
                    [Value::Content(body.clone())],
                )?
                .display())
        });
        let doc = result.output.expect("the template and body compile");

        let mut glyphs = Vec::new();
        for page in doc.pages() {
            spans(&page.frame, &mut glyphs);
        }
        let text: String = glyphs.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(text, "Hello");
        for (span, _) in &glyphs {
            match span.get() {
                SpanKind::Range { id, range } => {
                    assert_eq!(id, md);
                    assert_eq!(range, 2..7);
                }
                other => panic!("glyph span is not the Markdown range: {other:?}"),
            }
        }
    }
}
