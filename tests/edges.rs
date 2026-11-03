//! The places a writer's text meets the markup around it: a line break that
//! ends a table cell, a `;` after an emphasis, a self-referencing footnote.
//! Each compiles, sets the same pages through escaped markup as through the
//! words — which escaping cannot touch, so any difference is text the
//! markup misread — and keeps every word the writer wrote.

#![cfg(feature = "pdf")]

use std::path::Path;

use twig_export::config::Config;
use twig_export::pdf::Typesetter;
use twig_export::{Extensions, parse, render, render_words};

/// A source, and the words that must be set from it.
const CASES: &[(&str, &[&str])] = &[
    (
        "| a | b<br> |\n|---|---|\n| <br> | c<br> |\n",
        &["a", "b", "c"],
    ),
    (
        "*em*.name, **strong**(aside), `code`.rs",
        &["em.name", "strong(aside)", "code.rs"],
    ),
    (
        "*em*; **strong**; [link](u); `code`;",
        &["em;", "strong;", "link;", "code;"],
    ),
    (
        "A note[^1].after and[^1](again)\n\n[^1]: Body.",
        &[".after", "(again)", "Body"],
    ),
    ("a line\n2. is not a list item", &["a", "line", "2.", "is"]),
    (
        "Note: a term\n: Its definition",
        &["Note:", "term", "definition"],
    ),
    (
        "A heading\nover two lines\n===\n\nbody",
        &["heading", "over", "body"],
    ),
    ("A self[^1] reference.\n\n[^1]: See[^1].", &["self", "See"]),
    (
        "<https://e.com> and <a@e.com> and https://bare.com",
        &["https://e.com", "a@e.com", "https://bare.com"],
    ),
    ("a ~~struck~~ b", &["struck"]),
];

#[test]
fn every_edge_sets_the_same_pages_and_every_word() {
    let ts = Typesetter::new();
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/examples"));
    let format = twig_markdown::format();
    let cfg = Config::default();
    let ext = Extensions::default();
    for (source, words) in CASES {
        let typst = render(source, format, &cfg, ext).unwrap().typst;
        let markup = ts
            .pdf(&typst, root)
            .unwrap_or_else(|e| panic!("{source:?} does not compile: {e:#}\n{typst}"));
        let set = render_words(parse(source, format, ext).unwrap(), source, &cfg).unwrap();
        let layout = ts
            .lay_out(&set, root)
            .unwrap_or_else(|e| panic!("{source:?} does not set as words: {e:#}"));
        assert!(
            layout.pdf().unwrap() == markup,
            "{source:?}: the markup's pages are not the words'\n{typst}"
        );
        let text: String = layout
            .pages()
            .iter()
            .flat_map(|p| p.glyphs.iter().map(|g| g.text.as_str()))
            .collect::<String>()
            .split_whitespace()
            .collect();
        for word in *words {
            assert!(text.contains(word), "{source:?} lost {word:?}: {text}");
        }
    }
}
