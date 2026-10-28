//! The words set as content (`render_words`, `Typesetter::lay_out`) against
//! the same document set through escaped markup (`render`,
//! `Typesetter::pdf`): the same pages, byte for byte, and each glyph led
//! back to the source it was set from.

#![cfg(feature = "pdf")]

use std::path::Path;

use twig_export::config::Config;
use twig_export::pdf::{Layout, Typesetter};
use twig_export::{Extensions, parse, render, render_words};

/// Every construct `tests/pdf.rs` compiles, and the characters Typst would
/// read as markup if they were not escaped.
const CONSTRUCTS: &str = "\
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

Text with characters that must be escaped: # $ * _ ` < > @ ~ [ ] = - + / \\\" '

\"Quoted\" -- and dashed --- with an ellipsis...

[^1]: Footnote body.
";

fn examples() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples"))
}

/// The document set both ways: the escaped markup's PDF, and the words'
/// layout.
fn both(source: &str, cfg: &Config, ext: Extensions) -> (Vec<u8>, Layout) {
    let ts = Typesetter::new();
    let format = twig_markdown::format();
    let typst = render(source, format, cfg, ext).unwrap().typst;
    let markup = ts.pdf(&typst, examples()).expect("the markup sets");
    let words = render_words(parse(source, format, ext).unwrap(), source, cfg).unwrap();
    let layout = ts.lay_out(&words, examples()).expect("the words set");
    (markup, layout)
}

fn example(name: &str) -> (String, Config) {
    let source = std::fs::read_to_string(examples().join(format!("{name}.md"))).unwrap();
    let (cfg, _) = Config::load(&examples().join(format!("{name}.toml"))).unwrap();
    (source, cfg)
}

const DIRECTIVES: Extensions = Extensions {
    directives: true,
    math: false,
    html_elements: false,
};

/// How many of the layout's glyphs name a place in the source, and how many
/// of those name the bytes of their own text.
fn traced(layout: &Layout, source: &str) -> (usize, usize, usize) {
    let (mut all, mut named, mut exact) = (0, 0, 0);
    for page in layout.pages() {
        for g in page.glyphs {
            all += 1;
            if let Some(r) = g.source {
                named += 1;
                if source.get(r) == Some(g.text.as_str()) {
                    exact += 1;
                }
            }
        }
    }
    (all, named, exact)
}

#[test]
fn the_constructs_set_as_words_are_the_markups_pages() {
    let (markup, layout) = both(CONSTRUCTS, &Config::default(), Extensions::default());
    assert_eq!(layout.pdf().unwrap(), markup);
}

#[test]
fn the_report_set_as_words_is_the_markups_pages() {
    let (source, cfg) = example("report");
    let (markup, layout) = both(&source, &cfg, DIRECTIVES);
    assert_eq!(layout.pdf().unwrap(), markup);
}

/// The book's template is a Typst Universe package, which only `network`
/// resolves.
#[cfg(feature = "network")]
#[test]
fn the_book_set_as_words_is_the_markups_pages() {
    let (source, cfg) = example("folklore-book");
    let (markup, layout) = both(&source, &cfg, DIRECTIVES);
    assert_eq!(layout.pdf().unwrap(), markup);
}

/// A glyph of the author's text leads back to its own bytes. What names
/// nothing is what Typst draws itself (a list's numbers and bullets, a
/// footnote's number) or a template's own text (the report's title page,
/// its contents page, its page numbers).
#[test]
fn a_glyph_names_the_bytes_it_was_set_from() {
    let (_, layout) = both(CONSTRUCTS, &Config::default(), Extensions::default());
    let unnamed: String = layout
        .pages()
        .into_iter()
        .flat_map(|p| p.glyphs)
        .filter(|g| g.source.is_none())
        .map(|g| g.text)
        .collect();
    // The footnote's mark, the ordered list's numbers, a bullet and a space
    // before each item of the two lists, and the footnote's number at the
    // foot of the page.
    assert_eq!(unnamed, "11.2.• • • • 1");
    let (all, named, exact) = traced(&layout, CONSTRUCTS);
    assert_eq!(named, all - unnamed.chars().count());
    assert_eq!(exact, named, "every named glyph names its own bytes");

    let (source, cfg) = example("report");
    let (_, layout) = both(&source, &cfg, DIRECTIVES);
    let (_, named, exact) = traced(&layout, &source);
    assert!(
        exact * 100 >= named * 99,
        "{exact} of {named} name their own bytes"
    );
}

/// The heading's first glyph is the `H` of `# Heading`, at the page's top,
/// and the glyphs of a line run left to right along one baseline.
#[test]
fn a_glyph_is_where_the_page_draws_it() {
    let (_, layout) = both(
        "# Heading\n\nA line.\n",
        &Config::default(),
        Extensions::default(),
    );
    let pages = layout.pages();
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    let h = page
        .glyphs
        .iter()
        .find(|g| g.source == Some(2..3))
        .expect("the heading's H");
    assert_eq!(h.text, "H");
    assert!(h.x > 0.0 && h.x < page.width / 2.0, "{h:?}");
    assert!(h.baseline > 0.0 && h.baseline < page.height / 4.0, "{h:?}");
    assert!(h.ascent > 0.0 && h.descent > 0.0 && h.width > 0.0, "{h:?}");
    let line: Vec<_> = page
        .glyphs
        .iter()
        .filter(|g| g.source.as_ref().is_some_and(|r| r.start >= 11))
        .collect();
    assert_eq!(
        line.iter().map(|g| g.text.as_str()).collect::<String>(),
        "A line."
    );
    assert!(
        line.windows(2)
            .all(|w| w[0].x < w[1].x && w[0].baseline == w[1].baseline)
    );
    assert!(line[0].baseline > h.baseline);
}

/// A word's span counts from its block, so typing in the first block leaves
/// every word of the second where its span said it was.
#[test]
fn an_edit_in_one_block_leaves_the_next_blocks_spans() {
    let ts = Typesetter::new();
    let words = |source: &str| {
        let td = parse(source, twig_markdown::format(), Extensions::default()).unwrap();
        render_words(td, source, &Config::default()).unwrap()
    };
    let before = words("First.\n\nSecond.\n");
    let after = words("First, longer.\n\nSecond.\n");
    assert_eq!(before.typst, after.typst, "the holes are the same");
    let second = |w: &twig_export::Words, src: &str| {
        let layout = ts.lay_out(w, examples()).unwrap();
        let g = layout.pages()[0]
            .glyphs
            .iter()
            .find(|g| g.text == "S")
            .cloned()
            .unwrap();
        (g.source.clone().map(|r| src[r].to_string()), g.source)
    };
    let (a, ra) = second(&before, "First.\n\nSecond.\n");
    let (b, rb) = second(&after, "First, longer.\n\nSecond.\n");
    assert_eq!((a.as_deref(), b.as_deref()), (Some("S"), Some("S")));
    assert_eq!(rb.unwrap().start - ra.unwrap().start, ", longer".len());
}

/// A run longer than any word, with nowhere to break, breaks inside itself
/// rather than run off the page: on more than one line, and no glyph past
/// the page's right margin; set as words, each glyph still names its own
/// bytes, and the markup sets the same pages.
#[test]
fn a_run_too_long_for_a_line_breaks_inside_it() {
    let source = format!("This is a test{} and more words.\n", "!".repeat(300));
    let (markup, layout) = both(&source, &Config::default(), Extensions::default());
    assert_eq!(layout.pdf().unwrap(), markup);
    let pages = layout.pages();
    let page = &pages[0];
    let named: Vec<_> = page.glyphs.iter().filter(|g| g.source.is_some()).collect();
    let right = named.iter().map(|g| g.x + g.width).fold(0.0, f64::max);
    assert!(right <= page.width, "a glyph at {right}, past the page");
    let lines: std::collections::BTreeSet<i64> =
        named.iter().map(|g| g.baseline.round() as i64).collect();
    assert!(lines.len() >= 3, "{} lines", lines.len());
    let (_, named, exact) = traced(&layout, &source);
    assert_eq!(exact, named);
}
