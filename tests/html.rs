//! The HTML analogue of `tests/pdf.rs`: proves the emitted Typst actually
//! compiles through Typst's experimental HTML backend, not just the PDF one.
//! Content that only that backend drops (see `src/html.rs`) would pass the
//! golden test and `tests/pdf.rs` and still break here.

#![cfg(feature = "html")]

use std::path::Path;

use twig_export::{config::Config, html, render};

#[test]
fn the_example_compiles_to_real_html() {
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

    let doc = html::compile(&rendered.typst).expect("the generated Typst must compile");

    assert!(doc.starts_with("<!DOCTYPE html>"), "output must be HTML");
    assert!(
        doc.contains("Quarterly Field Notes"),
        "the title must survive HTML export, not just PDF — \
         `#align(..)[..]` used to drop its content under this backend"
    );
}

/// Every construct twig can parse must survive the round trip to HTML, not
/// just the ones the example happens to use.
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

    html::compile(&rendered.typst).expect("every construct must produce compilable Typst");
}
