//! Objects set as words (`render_words_with`, `Typesetter::lay_out`): a
//! picture where the host found it, or its alt text where there is none,
//! and the room a host asks for under a block, each one glyph as big as it
//! is, set from the object's bytes of the source.

#![cfg(feature = "pdf")]

use std::path::Path;
use std::sync::Arc;

use twig_export::config::Config;
use twig_export::pdf::{Glyph, Layout, OBJECT, Typesetter};
use twig_export::{Extensions, Hooks, Picture, parse, render_words_with};

/// A picture 40 points wide and 20 tall, as an SVG.
const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40pt" height="20pt"><rect width="40" height="20" fill="red"/></svg>"#;

struct Host {
    found: Vec<(&'static str, Picture)>,
    rooms: Vec<(usize, f64)>,
}

impl Hooks for Host {
    fn picture(&self, destination: &str) -> Picture {
        self.found
            .iter()
            .find(|(d, _)| *d == destination)
            .map_or(Picture::Missing, |(_, p)| p.clone())
    }

    fn room(&self, at: usize) -> Option<f64> {
        self.rooms.iter().find(|(a, _)| *a == at).map(|(_, h)| *h)
    }
}

fn set(source: &str, host: &Host) -> Layout {
    let format = twig_markdown::format();
    let ext = Extensions {
        directives: true,
        ..Extensions::default()
    };
    let mut cfg = Config::default();
    cfg.document.page_width = Some("300pt".into());
    cfg.document.page_height = Some("400pt".into());
    let words = render_words_with(parse(source, format, ext).unwrap(), source, &cfg, host).unwrap();
    Typesetter::new()
        .lay_out(&words, Path::new("/nonexistent"))
        .expect("the document sets")
}

fn objects(layout: &Layout) -> Vec<Glyph> {
    layout
        .pages()
        .into_iter()
        .flat_map(|p| p.glyphs)
        .filter(|g| g.text == OBJECT)
        .collect()
}

fn text(layout: &Layout) -> String {
    layout
        .pages()
        .into_iter()
        .flat_map(|p| p.glyphs)
        .filter(|g| g.text != OBJECT)
        .map(|g| g.text)
        .collect()
}

fn svg() -> Picture {
    Picture::Bytes(Arc::from(SVG.as_bytes()))
}

#[test]
fn a_picture_the_host_found_is_one_object_its_size() {
    let src = "Above\n\n![a box](box.svg)\n\nBelow\n";
    let layout = set(
        src,
        &Host {
            found: vec![("box.svg", svg())],
            rooms: vec![],
        },
    );
    let found = objects(&layout);
    assert_eq!(found.len(), 1, "{found:?}");
    let g = &found[0];
    assert_eq!(&src[g.source.clone().unwrap()], "![a box](box.svg)");
    assert!((g.width - 40.0).abs() < 0.01, "{g:?}");
    assert!((g.ascent - 20.0).abs() < 0.01, "{g:?}");
    assert_eq!(g.descent, 0.0);
    // Between the paragraphs.
    let line = |w: &str| {
        let at = src.find(w).unwrap();
        layout
            .pages()
            .into_iter()
            .flat_map(|p| p.glyphs)
            .find(|g| g.source.as_ref().is_some_and(|r| r.start == at))
            .unwrap()
            .baseline
    };
    assert!(line("Above") < g.baseline - g.ascent);
    assert!(line("Below") > g.baseline);
}

#[test]
fn a_picture_there_is_none_of_is_its_alt_text() {
    let layout = set(
        "![the missing one](gone.png)\n",
        &Host {
            found: vec![],
            rooms: vec![],
        },
    );
    assert!(objects(&layout).is_empty());
    assert_eq!(text(&layout), "the missing one");
}

#[test]
fn a_data_url_is_a_file_beside_the_source() {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(SVG);
    let src = format!("![a box](data:image/svg+xml;base64,{b64})\n");
    let format = twig_markdown::format();
    let words = render_words_with(
        parse(&src, format, Extensions::default()).unwrap(),
        &src,
        &Config::default(),
        &Host {
            found: vec![],
            rooms: vec![],
        },
    )
    .unwrap();
    assert_eq!(words.files.len(), 1);
    assert!(!words.typst.contains("bytes(("), "{}", words.typst);
    let layout = Typesetter::new().lay_out(&words, Path::new(".")).unwrap();
    assert_eq!(objects(&layout).len(), 1);
}

#[test]
fn a_picture_as_written_is_read_against_the_root_and_is_an_object() {
    let dir = std::env::temp_dir().join(format!("twig-export-objects-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("box.svg"), SVG).unwrap();
    let src = "![a box](box.svg)\n";
    let words = twig_export::render_words(
        parse(src, twig_markdown::format(), Extensions::default()).unwrap(),
        src,
        &Config::default(),
    )
    .unwrap();
    let layout = Typesetter::new().lay_out(&words, &dir);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(objects(&layout.unwrap()).len(), 1);
}

#[test]
fn a_room_is_left_under_a_block_and_is_an_object() {
    let src = "Above\n\n::card[A label]{x=\"1\"}\n\nBelow\n";
    let at = src.find("::card").unwrap();
    let layout = set(
        src,
        &Host {
            found: vec![],
            rooms: vec![(at, 50.0)],
        },
    );
    let found = objects(&layout);
    assert_eq!(found.len(), 1, "{found:?}");
    let g = &found[0];
    assert_eq!(&src[g.source.clone().unwrap()], "::card[A label]{x=\"1\"}");
    assert!((g.ascent - 50.0).abs() < 0.01, "{g:?}");
    assert!(g.width > 100.0, "as wide as the column: {g:?}");
    // The label above it, the next paragraph below.
    let below = src.find("Below").unwrap();
    let label = src.find("A label").unwrap();
    let glyphs: Vec<Glyph> = layout.pages().into_iter().flat_map(|p| p.glyphs).collect();
    let at = |b: usize| {
        glyphs
            .iter()
            .find(|g| g.source.as_ref().is_some_and(|r| r.start == b))
            .unwrap()
            .baseline
    };
    assert!(at(label) < g.baseline - g.ascent);
    assert!(at(below) > g.baseline);
}
