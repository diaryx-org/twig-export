//! The one test that proves the emitted Typst is actually *valid* Typst.
//!
//! The golden test pins what we emit; this pins that what we emit compiles.
//! Those are different failures: a mapping bug that produces plausible-looking
//! but unparseable markup passes the golden test the moment the golden file is
//! regenerated, and only this catches it.

#![cfg(feature = "pdf")]

use std::path::Path;

use twig_export::{config::Config, pdf, render};

#[test]
fn the_example_compiles_to_a_real_pdf() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("examples/report.md")).unwrap();
    let (cfg, _) = Config::load(&root.join("examples/report.toml")).unwrap();

    let rendered = render(
        &source,
        twig_markdown::format(),
        &cfg,
        twig_export::Extensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    let bytes = pdf::compile(&rendered.typst).expect("the generated Typst must compile");

    assert!(bytes.starts_with(b"%PDF-"), "output must be a PDF");
    assert!(bytes.len() > 1000, "a one-page document should not be tiny");
}

/// Every construct twig can parse must survive the round trip to PDF, not just
/// the ones the example happens to use.
#[test]
fn every_supported_construct_compiles() {
    let source = "\
# Heading

Para with *em*, **strong**, `code`, [link](https://example.com), and a footnote[^1].

> A quote.

1) one
2) two

- bullet
- another

- [ ] todo
- [x] done

Term
: Definition

| a | b |
|:--|--:|
| 1 | 2 |

```rust
fn main() {}
```

---

Text with characters that must be escaped: # $ * _ ` < > @ ~ [ ] = - + / \" '

[^1]: Footnote body.
";
    let rendered = render(
        source,
        twig_markdown::format(),
        &Config::default(),
        twig_export::Extensions::default(),
    )
    .unwrap();

    pdf::compile(&rendered.typst).expect("every construct must produce compilable Typst");
}

/// `compile_in` resolves a picture against the document's folder, not the
/// working directory: an editor has a document open wherever it was
/// launched.
#[test]
fn a_picture_resolves_against_the_folder_compile_in_names() {
    let dir = std::env::temp_dir().join(format!("twig-export-compile-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("dot.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle cx="5" cy="5" r="4"/></svg>"#,
    )
    .unwrap();
    let rendered = render(
        "A dot:\n\n![a dot](dot.svg)\n",
        twig_markdown::format(),
        &Config::default(),
        Default::default(),
    )
    .unwrap();

    let in_folder = pdf::compile_in(&rendered.typst, &dir);
    let elsewhere = pdf::compile_in(&rendered.typst, &dir.join("missing"));
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(
        in_folder
            .expect("the picture is beside the document")
            .starts_with(b"%PDF-")
    );
    assert!(
        elsewhere.is_err(),
        "a picture not in the root must not be found"
    );
}

/// `render_parsed` takes a document the caller parsed with features of its
/// own, including ones twig-export's `Extensions` does not name.
#[test]
fn a_document_parsed_by_the_caller_renders_and_compiles() {
    let source = "# Notes\n\nSome ==marked== text and $x^2$ math.\n";
    let td = twig::Document::parse_with_features(
        source.as_bytes(),
        twig_markdown::format(),
        &["highlight", "math", "directives"],
    )
    .unwrap();
    let rendered = twig_export::render_parsed(td, source, &Config::default()).unwrap();
    assert!(rendered.typst.contains("Notes"));
    assert!(rendered.typst.contains("marked"));
    let bytes = pdf::compile(&rendered.typst).expect("a caller-parsed document compiles");
    assert!(bytes.starts_with(b"%PDF-"));
}
