//! The `typst::World` implementation shared by the PDF and HTML backends.
//!
//! Both backends compile the same single in-memory source with the same font
//! and file-resolution rules; the only thing that differs between them is
//! which in-development [`Features`] the library is built with (HTML export
//! requires `Feature::Html`, PDF requires none) and which document type they
//! ask `typst::compile` for.
//!
//! ── Files and packages ──────────────────────────────────────────────────────
//! A document's own files (its pictures) resolve against its folder, and a
//! path that leaves the folder, by `..` or through a symlink, is refused.
//! A package's files resolve against the package's own folder, under the
//! same rule: a template, being a package, reads only what it ships with,
//! never the document's folder. See [`Packages`] for where packages come
//! from.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use typst::diag::{FileError, FileResult, PackageError};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::package::PackageSpec;
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Features, Library, LibraryExt, World};
use typst_kit::fonts::FontStore;

use crate::templates;

/// Where a world finds the packages a source imports: the templates
/// twig-export ships, from memory; then each folder given, laid out as
/// Typst lays out its own (`namespace/name/version/`), in order. With the
/// `network` feature, then Typst's own package folders, and Typst Universe,
/// downloading as typst-cli does; without it, never the network: a package
/// is installed first ([`crate::install`]).
#[derive(Clone, Debug, Default)]
pub struct Packages {
    dirs: Vec<PathBuf>,
}

impl Packages {
    /// The built-in templates, and the packages in each of `dirs`.
    pub fn new(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        Packages {
            dirs: dirs.into_iter().collect(),
        }
    }

    /// The folders searched, in order.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// The file at `path` in the package `spec`.
    fn load(&self, spec: &PackageSpec, path: &VirtualPath) -> FileResult<Bytes> {
        if spec.namespace == templates::NAMESPACE {
            let builtin = templates::builtin(&spec.name)
                .filter(|_| spec.version.to_string() == templates::VERSION);
            let Some(t) = builtin else {
                return Err(PackageError::NotFound(spec.clone()).into());
            };
            return match path.get_without_slash() {
                "typst.toml" => Ok(Bytes::from_string(t.manifest)),
                "lib.typ" => Ok(Bytes::from_string(t.source)),
                other => Err(FileError::NotFound(other.into())),
            };
        }
        let at = |dir: &Path| {
            dir.join(spec.namespace.as_str())
                .join(spec.name.as_str())
                .join(spec.version.to_string())
        };
        if let Some(dir) = self.dirs.iter().map(|d| at(d)).find(|d| d.is_dir()) {
            return confined(&dir, path);
        }
        #[cfg(feature = "network")]
        {
            use typst_kit::{downloader::SystemDownloader, packages::SystemPackages};
            static SYSTEM: std::sync::OnceLock<SystemPackages> = std::sync::OnceLock::new();
            let system = SYSTEM.get_or_init(|| {
                SystemPackages::new(SystemDownloader::new(concat!(
                    "twig-export/",
                    env!("CARGO_PKG_VERSION")
                )))
            });
            let root = system.obtain(spec)?;
            confined(root.path(), path)
        }
        #[cfg(not(feature = "network"))]
        Err(PackageError::NotFound(spec.clone()).into())
    }
}

/// The file at `path` under `root`, read, unless it is not under `root`
/// once every symlink on the way is followed.
pub(crate) fn confined(root: &Path, path: &VirtualPath) -> FileResult<Bytes> {
    let joined = path.realize(root)?;
    fn io(p: &Path) -> impl Fn(std::io::Error) -> FileError + '_ {
        move |e| FileError::from_io(e, p)
    }
    let real = std::fs::canonicalize(&joined).map_err(io(&joined))?;
    let base = std::fs::canonicalize(root).map_err(io(root))?;
    if !real.starts_with(&base) {
        return Err(FileError::AccessDenied);
    }
    if real.is_dir() {
        return Err(FileError::IsDirectory);
    }
    std::fs::read(&real).map(Bytes::new).map_err(io(&real))
}

pub(crate) struct ExportWorld {
    library: LazyHash<Library>,
    fonts: Arc<FontStore>,
    main: Source,
    /// The folder a document's relative paths (its pictures) resolve against.
    root: PathBuf,
    packages: Packages,
    /// Every file read once, for the compile's many asks.
    files: Mutex<HashMap<FileId, FileResult<Bytes>>>,
    sources: Mutex<HashMap<FileId, FileResult<Source>>>,
}

impl ExportWorld {
    /// A world over `typ` whose relative paths resolve against the working
    /// directory.
    pub(crate) fn new(typ: &str, features: Features) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        Self::rooted(typ, features, &cwd)
    }

    /// A world over `typ` whose relative paths resolve against `root`: the
    /// document's own folder, for an editor that has a document open
    /// wherever it was launched.
    pub(crate) fn rooted(typ: &str, features: Features, root: &Path) -> Self {
        Self::with_fonts(typ, features, root, Arc::new(fonts()))
    }

    /// A world over `typ` rooted at `root`, in `fonts` read already: for a
    /// caller that compiles again and again and reads the system's fonts
    /// once.
    pub(crate) fn with_fonts(
        typ: &str,
        features: Features,
        root: &Path,
        fonts: Arc<FontStore>,
    ) -> Self {
        let vpath = VirtualPath::new("main.typ").expect("main.typ is a valid virtual path");
        let id = RootedPath::new(VirtualRoot::Project, vpath).intern();

        let library = Library::builder().with_features(features).build();

        ExportWorld {
            library: LazyHash::new(library),
            fonts,
            main: Source::new(id, typ.into()),
            root: root.to_path_buf(),
            packages: Packages::default(),
            files: Mutex::default(),
            sources: Mutex::default(),
        }
    }

    /// This world, finding packages in `packages`.
    pub(crate) fn with_packages(mut self, packages: Packages) -> Self {
        self.packages = packages;
        self
    }

    /// The generated source text, for quoting the offending line in a
    /// compile-error message.
    pub(crate) fn main_text(&self) -> &str {
        self.main.text()
    }

    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => confined(&self.root, id.vpath()),
            VirtualRoot::Package(spec) => self.packages.load(spec, id.vpath()),
        }
    }
}

/// The fonts a document is set in: embedded first, then the system's. Order
/// is precedence, so a document renders identically on a machine with no
/// fonts installed and a machine with a thousand — the embedded Libertinus /
/// New Computer Modern / DejaVu set always wins for the names it provides.
pub(crate) fn fonts() -> FontStore {
    let mut fonts = FontStore::new();
    fonts.extend(typst_kit::fonts::embedded());
    fonts.extend(typst_kit::fonts::system());
    fonts
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
            return Ok(self.main.clone());
        }
        let mut sources = self.sources.lock().expect("the sources lock");
        sources
            .entry(id)
            .or_insert_with(|| {
                let bytes = self.file(id)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| FileError::InvalidUtf8)?;
                Ok(Source::new(id, text.into()))
            })
            .clone()
    }

    /// Besides the packages a source imports, the only files Typst asks for
    /// are the pictures a document links to and the files a package reads
    /// of its own.
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let mut files = self.files.lock().expect("the files lock");
        files.entry(id).or_insert_with(|| self.load(id)).clone()
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
