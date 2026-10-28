//! The templates twig-export ships, set: each in both layouts, light and
//! dark, through the markup and through the words alike.

#![cfg(feature = "pdf")]

use std::path::Path;

use twig_export::config::Config;
use twig_export::pdf::{Layout, Typesetter};
use twig_export::templates::NAMES;
use twig_export::{Extensions, parse, render, render_words};

/// Every construct the emitter makes, so a template styles each.
const CONSTRUCTS: &str = "\
# A title

A paragraph with *emphasis*, **strong**, `code`, [a link](https://example.com),
and a footnote[^1]. It goes on long enough to wrap across a line or two, so
that the leading and the measure show.

## A section

> A quotation, set apart.

1. One
2. Two

- A bullet
- Another

### A subsection

| Name | Qty |
|------|----:|
| Pear | 3 |

```rust
fn main() {}
```

---

A last paragraph.

[^1]: The note.
";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn config(template: &str, layout: &str, dark: bool) -> Config {
    let mut cfg = Config::default();
    cfg.document.template = Some(template.into());
    cfg.document.layout = Some(layout.into());
    cfg.document.dark = Some(dark);
    if layout == "reflow" {
        cfg.document.page_width = Some("480pt".into());
    }
    cfg
}

/// The document set as markup and as words, both under `cfg`.
fn both(ts: &Typesetter, cfg: &Config) -> (Vec<u8>, Layout) {
    let format = twig_markdown::format();
    let ext = Extensions::default();
    let typst = render(CONSTRUCTS, format, cfg, ext).unwrap().typst;
    let markup = ts.pdf(&typst, root()).expect("the markup sets");
    let words = render_words(parse(CONSTRUCTS, format, ext).unwrap(), CONSTRUCTS, cfg).unwrap();
    let layout = ts.lay_out(&words, root()).expect("the words set");
    (markup, layout)
}

#[test]
fn every_template_sets_in_both_layouts_light_and_dark() {
    let ts = Typesetter::new();
    for name in NAMES {
        for layout in ["paged", "reflow"] {
            for dark in [false, true] {
                let (markup, set) = both(&ts, &config(name, layout, dark));
                assert_eq!(set.pdf().unwrap(), markup, "{name} {layout} dark {dark}");
                let mut named = 0;
                for page in set.pages() {
                    for g in page.glyphs {
                        if let Some(r) = g.source {
                            named += 1;
                            // A template may set a run in capitals (Editorial's
                            // sections): the glyph is its byte's, in another case.
                            let byte = CONSTRUCTS.get(r.clone()).unwrap_or_default();
                            assert!(
                                byte.to_lowercase() == g.text.to_lowercase(),
                                "{name}: {byte:?} {:?}",
                                g.text
                            );
                        }
                    }
                }
                assert!(named > 200, "{name} {layout}: {named} glyphs from the text");
            }
        }
    }
}

/// Reflowed, a document is one page as wide as asked and as tall as its
/// text: shorter than a sheet for a short one.
#[test]
fn a_reflowed_document_is_one_page_as_wide_as_asked() {
    let ts = Typesetter::new();
    for name in NAMES {
        let (_, set) = both(&ts, &config(name, "reflow", false));
        let pages = set.pages();
        assert_eq!(pages.len(), 1, "{name}");
        assert_eq!(pages[0].width, 480.0, "{name}");
        let last = pages[0]
            .glyphs
            .iter()
            .map(|g| g.baseline)
            .fold(0.0, f64::max);
        assert!(
            pages[0].height > last && pages[0].height < 1200.0,
            "{name}: {}",
            pages[0].height
        );
    }
}

#[test]
fn a_template_twig_export_does_not_have_is_refused() {
    let err = render(
        CONSTRUCTS,
        twig_markdown::format(),
        &config("nonesuch", "paged", false),
        Extensions::default(),
    )
    .err()
    .expect("refused");
    assert!(
        err.to_string().contains("classic, modern, editorial"),
        "{err:#}"
    );
}

#[test]
fn every_builtin_passes_the_check_an_editor_makes() {
    let ts = Typesetter::new();
    for name in NAMES {
        let manifest = twig_export::templates::builtin(name).unwrap().manifest();
        ts.check(name, &manifest.twig().unwrap().layouts)
            .unwrap_or_else(|e| panic!("{name}: {e:#}"));
    }
}
