//! Templates as packages: one on disk is found and set, given the
//! frontmatter its config lists, and kept to its own files; a document is
//! kept to its own folder.

#![cfg(feature = "pdf")]

use std::fs;
use std::path::{Path, PathBuf};

use twig_export::config::Config;
use twig_export::pdf::{Packages, Typesetter};
use twig_export::templates::{Spec, installed};
use twig_export::{Extensions, parse, render, render_words};

/// A scratch folder, made empty, under the target directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A template package `@local/<name>:1.0.0` under `packages`, its
/// entrypoint `lib`.
fn package(packages: &Path, name: &str, lib: &str) -> PathBuf {
    let dir = packages.join("local").join(name).join("1.0.0");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("typst.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nentrypoint = \"lib.typ\"\n\n\
             [tool.twig]\nlabel = \"Plain\"\nlayouts = [\"paged\"]\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("lib.typ"), lib).unwrap();
    dir
}

fn config(template: &str) -> Config {
    let mut cfg = Config::default();
    cfg.document.template = Some(template.into());
    cfg.document.title_page = Some(String::new());
    cfg
}

/// `source` set under `cfg` in `root`, through the words, as its text.
fn text(ts: &Typesetter, cfg: &Config, source: &str, root: &Path) -> anyhow::Result<String> {
    let format = twig_markdown::format();
    let words = render_words(parse(source, format, Extensions::default())?, source, cfg)?;
    let layout = ts.lay_out(&words, root)?;
    Ok(layout
        .pages()
        .iter()
        .flat_map(|p| p.glyphs.iter().map(|g| g.text.clone()))
        .collect())
}

#[test]
fn a_template_on_disk_is_found_and_given_the_listed_frontmatter() {
    let dir = scratch("packages-found");
    let packages = dir.join("packages");
    package(
        &packages,
        "plain",
        "#let twig-template(body, meta: (:), ..rest) = {\n\
         [Title: #meta.at(\"title\", default: \"none\")]\n\
         [Hidden: #meta.at(\"secret\", default: \"no\")]\n\
         body\n}\n",
    );
    let found = installed(&packages);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].spec, Spec::parse("@local/plain:1.0.0").unwrap());
    assert_eq!(found[0].manifest.twig().unwrap().label, "Plain");

    let ts = Typesetter::new().with_packages(Packages::new([packages]));
    let mut cfg = config("@local/plain:1.0.0");
    cfg.document.template_meta = Some(vec!["title".into()]);
    let doc = "---\ntitle: \"#panic()\"\nsecret: shh\n---\n\nBody.\n";
    let text = text(&ts, &cfg, doc, &dir).expect("the template sets");
    // The title arrives as a string, never run as code; a key the config
    // does not list never arrives.
    assert!(text.contains("Title: #panic()"), "{text}");
    assert!(text.contains("Hidden: no"), "{text}");
    assert!(text.contains("Body."), "{text}");
}

#[test]
fn a_template_not_installed_is_not_found() {
    let dir = scratch("packages-missing");
    let ts = Typesetter::new().with_packages(Packages::new([dir.join("packages")]));
    let err = text(&ts, &config("@local/absent:1.0.0"), "Body.\n", &dir).unwrap_err();
    assert!(format!("{err:#}").contains("not found"), "{err:#}");
}

#[test]
fn a_template_reads_only_its_own_files() {
    let dir = scratch("packages-confined");
    let packages = dir.join("packages");
    fs::write(dir.join("private.md"), "the reader's own note").unwrap();
    let ts = Typesetter::new().with_packages(Packages::new([packages.clone()]));

    // A path in a package is the package's, however it is written.
    for (name, path) in [
        ("rooted", "/private.md"),
        ("climbing", "../../../../private.md"),
    ] {
        let lib = format!("#let twig-template(body, ..rest) = {{ read({path:?}); body }}\n");
        package(&packages, name, &lib);
        let spec = format!("@local/{name}:1.0.0");
        let err = text(&ts, &config(&spec), "Body.\n", &dir).unwrap_err();
        assert!(!format!("{err:#}").contains("reader's own"), "{name}");
    }

    // And a symlink in a package does not lead out of it.
    let lib = "#let twig-template(body, ..rest) = { read(\"link.md\"); body }\n";
    let pkg = package(&packages, "linked", lib);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("private.md"), pkg.join("link.md")).unwrap();
        let err = text(&ts, &config("@local/linked:1.0.0"), "Body.\n", &dir).unwrap_err();
        assert!(format!("{err:#}").contains("access denied"), "{err:#}");
    }
}

#[cfg(unix)]
#[test]
fn a_document_reads_only_its_own_folder() {
    let dir = scratch("packages-document");
    let outside = dir.join("outside.typ");
    fs::write(&outside, "secret").unwrap();
    let folder = dir.join("doc");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("inside.txt"), "kept").unwrap();
    std::os::unix::fs::symlink(&outside, folder.join("link.txt")).unwrap();

    let ts = Typesetter::new();
    assert!(ts.pdf("#read(\"inside.txt\")", &folder).is_ok());
    let err = ts.pdf("#read(\"link.txt\")", &folder).unwrap_err();
    assert!(format!("{err:#}").contains("access denied"), "{err:#}");
}

#[test]
fn typst_for_typst_cli_carries_a_builtin_template() {
    let cfg = config("modern");
    let typst = render(
        "Body.\n",
        twig_markdown::format(),
        &cfg,
        Extensions::default(),
    )
    .unwrap()
    .typst;
    assert!(typst.starts_with("#import \"@twig/modern:0.1.0\": twig-template\n"));
    let inlined = twig_export::templates::inline(&typst);
    assert!(!inlined.contains("@twig/"));
    // Inlined, it sets as the import does.
    let ts = Typesetter::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        ts.pdf(&typst, root).unwrap(),
        ts.pdf(&inlined, root).unwrap()
    );
}
