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
