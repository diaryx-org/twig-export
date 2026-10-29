//! Tiles: a page cut into a block's each, keyed by what each draws.

#![cfg(feature = "svg")]

use std::collections::HashSet;
use std::path::Path;

use twig_export::config::Config;
use twig_export::pdf::{Layout, Typesetter};
use twig_export::{Extensions, parse, render_words};

fn set(ts: &Typesetter, source: &str, cfg: &Config) -> Layout {
    let format = twig_markdown::format();
    let words = render_words(
        parse(source, format, Extensions::default()).unwrap(),
        source,
        cfg,
    )
    .unwrap();
    ts.lay_out(&words, Path::new(".")).unwrap()
}

fn reflowed() -> Config {
    let mut cfg = Config::default();
    cfg.document.template = Some("classic".into());
    cfg.document.layout = Some("reflow".into());
    cfg.document.page_width = Some("400pt".into());
    cfg
}

const DOC: &str = "# Title\n\nFirst paragraph.\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\nLast paragraph.\n";

fn keys(layout: &Layout) -> Vec<u128> {
    layout.tiles().iter().map(|t| t.key).collect()
}

#[test]
fn a_block_is_a_tile_and_every_glyph_is_on_one() {
    let ts = Typesetter::new();
    let layout = set(&ts, DOC, &reflowed());
    let tiles = layout.tiles();
    let blocks: HashSet<_> = tiles.iter().filter_map(|t| t.block).collect();
    // Title, two paragraphs, the list, the table, and the rule drawn with a
    // block beside it.
    assert!(blocks.len() >= 5, "{} blocks", blocks.len());
    assert_eq!(blocks.len(), tiles.len(), "a tile a block");
    for t in &tiles {
        assert!(t.height > 0.0 && t.width == 400.0);
        assert!(layout.tile_svg(t).starts_with("<svg"));
    }
    // Every glyph lies within a tile of its page.
    for (i, page) in layout.pages().iter().enumerate() {
        for g in &page.glyphs {
            assert!(
                tiles
                    .iter()
                    .any(|t| t.page == i && g.baseline >= t.top && g.baseline <= t.top + t.height),
                "{:?} at {}",
                g.text,
                g.baseline
            );
        }
    }
}

#[test]
fn an_edit_changes_its_blocks_tile_and_a_block_moved_keeps_its_key() {
    let ts = Typesetter::new();
    let before: HashSet<u128> = keys(&set(&ts, DOC, &reflowed())).into_iter().collect();
    let edited = DOC.replace("First paragraph.", "First paragraph, longer.");
    let after = keys(&set(&ts, &edited, &reflowed()));
    let changed = after.iter().filter(|k| !before.contains(k)).count();
    assert_eq!(changed, 1, "only the edited block draws anew");

    // A paragraph put in above everything: every block below moves, and is
    // the same picture where it is.
    let inserted = DOC.replace("# Title\n\n", "# Title\n\nA new paragraph.\n\n");
    let after = keys(&set(&ts, &inserted, &reflowed()));
    let new = after.iter().filter(|k| !before.contains(k)).count();
    assert_eq!(new, 1, "only the new block draws anew");
}

/// An underline under a paragraph's last line is drawn below the line,
/// outside the paragraph's frame: its tile reaches down to it.
#[test]
fn a_tile_holds_what_is_drawn_past_its_blocks_frame() {
    use twig_export::config::Rule;
    use twig_export::style::Style;
    let ts = Typesetter::new();
    let mut cfg = reflowed();
    cfg.document.page_width = Some("300pt".into());
    cfg.document.size = Some("12pt".into());
    let underline = Style {
        underline: Some(true),
        ..Style::default()
    };
    let strike = Style {
        strike: Some(true),
        ..Style::default()
    };
    for (select, style) in [("strong", underline), ("emph", strike)] {
        cfg.rules.push(Rule {
            select: select.into(),
            style,
        });
    }
    let src =
        "Plain and *struck* and marked and H*2*O and x and **html u** html s m.\n\nThe next.\n";
    let l = set(&ts, src, &cfg);
    let t = &l.tiles()[0];
    let svg = l.tile_svg(t);
    // Each line drawn (a strike, an underline) is a path moved to its place.
    let lines: Vec<f64> = svg
        .split("transform=\"translate(")
        .skip(1)
        .filter(|rest| rest.contains(")\" d=\"M 0 0h"))
        .filter_map(|rest| rest.split([' ', ')']).nth(1)?.parse().ok())
        .collect();
    assert_eq!(lines.len(), 3, "{svg}");
    for y in lines {
        assert!(y < t.height, "a line at {y}, below the tile's {}", t.height);
    }
}
