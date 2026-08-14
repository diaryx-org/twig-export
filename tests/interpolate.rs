//! `{{...}}` interpolation in raw-Typst config fields — the fix for "one rule
//! per value" (see `src/typst_emit.rs`'s `interpolate`/`interpolate_node`/
//! `interpolate_doc`).
//!
//! These go through the real pipeline (`render`), not just the interpolation
//! function in isolation: the point being proven is that a single rule with a
//! placeholder now covers every value a document actually uses, which only
//! `render`'s match-count bookkeeping can show.

use twig_export::{config::Config, render};

/// One rule, `{{attr.count}}`, must cover every `count` a document uses —
/// that is the whole feature. Before interpolation this needed one rule per
/// distinct count (see `examples/folklore-book.toml`'s history).
#[test]
fn one_rule_with_a_placeholder_covers_every_attribute_value() {
    let source = "::lorem{count=150}\n\n::lorem{count=42}\n";
    let (cfg, warnings) = Config::parse(
        "[[rule]]\n\
         select = \"container[name=lorem]\"\n\
         raw.before = \"#lorem(int({{attr.count}}))\"\n",
        "toml",
    )
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");

    let rendered = render(
        source,
        twig::Format::Markdown,
        &cfg,
        twig::MarkdownExtensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(rendered.match_counts, vec![2], "one rule, both nodes matched");
    assert!(rendered.typst.contains("#lorem(int(\"150\"))"), "{}", rendered.typst);
    assert!(rendered.typst.contains("#lorem(int(\"42\"))"), "{}", rendered.typst);
}

/// `raw.wrap` is interpolated too — a wrapper's function/argument can vary
/// per node, not just `before`/`after`.
#[test]
fn wrap_is_interpolated_against_the_matched_node() {
    let source = "::note[This is the note body.]{severity=warn}\n";
    let (cfg, _) = Config::parse(
        "[[rule]]\n\
         select = \"container[name=note]\"\n\
         raw.wrap = \"note-box.with(severity: {{attr.severity}})\"\n",
        "toml",
    )
    .unwrap();

    let rendered = render(
        source,
        twig::Format::Markdown,
        &cfg,
        twig::MarkdownExtensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        rendered
            .typst
            .contains("#note-box.with(severity: \"warn\")["),
        "{}",
        rendered.typst
    );
}

/// `{{doc.title}}`/`{{doc.author}}` in `raw-preamble` read back the SAME
/// resolved values `#set document(...)` used — sourced from Rust-resolved
/// config, not Typst's own `context` (which does not compose safely with a
/// template that re-derives its own metadata from that value; see
/// `examples/folklore-book.toml`).
#[test]
fn document_fields_interpolate_into_raw_preamble() {
    let source = "---\ntitle: My Book\nauthor: A. Author\n---\n\nHello.\n";
    let (cfg, _) = Config::parse(
        "[document]\n\
         raw-preamble = \"#let manual-title = {{doc.title}}\\n#let manual-author = {{doc.author}}\"\n\
         [document.from-metadata]\n\
         title = \"title\"\n\
         author = \"author\"\n",
        "toml",
    )
    .unwrap();

    let rendered = render(
        source,
        twig::Format::Markdown,
        &cfg,
        twig::MarkdownExtensions::default(),
    )
    .unwrap();

    assert!(
        rendered.typst.contains("#let manual-title = \"My Book\""),
        "{}",
        rendered.typst
    );
    assert!(
        rendered.typst.contains("#let manual-author = \"A. Author\""),
        "{}",
        rendered.typst
    );
}

/// A placeholder that never matches (wrong attribute name, or a node that
/// simply lacks it) resolves to an empty Typst string rather than erroring —
/// same "renders anyway, config surface stays visible" posture as an unfired
/// `from-metadata` binding.
#[test]
fn an_unmatched_placeholder_resolves_to_an_empty_string_not_an_error() {
    let source = "::lorem\n";
    let (cfg, _) = Config::parse(
        "[[rule]]\n\
         select = \"container[name=lorem]\"\n\
         raw.before = \"#[count: {{attr.count}}]\"\n",
        "toml",
    )
    .unwrap();

    let rendered = render(
        source,
        twig::Format::Markdown,
        &cfg,
        twig::MarkdownExtensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(rendered.typst.contains("#[count: \"\"]"), "{}", rendered.typst);
}

#[cfg(feature = "pdf")]
#[test]
fn interpolated_output_still_compiles() {
    let source = "::lorem{count=150}\n";
    let (cfg, _) = Config::parse(
        "[[rule]]\n\
         select = \"container[name=lorem]\"\n\
         raw.before = \"#lorem(int({{attr.count}}))\"\n",
        "toml",
    )
    .unwrap();

    let rendered = render(
        source,
        twig::Format::Markdown,
        &cfg,
        twig::MarkdownExtensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    twig_export::pdf::compile(&rendered.typst).expect("interpolated Typst must compile");
}
