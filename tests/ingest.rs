//! Tests for the twig-facing seam.
//!
//! These are the tests that would catch a twig upgrade changing something this
//! crate depends on. Everything else is pure text-to-text and lives in unit
//! tests next to the code.

use twig_export::config::{Config, Rule};
use twig_export::doc::Doc;
use twig_export::style::Style;

fn parse(src: &str) -> (twig::Document, Doc) {
    let ext = twig::MarkdownExtensions {
        directives: true,
        ..Default::default()
    };
    let mut td = twig::Document::parse_str_with(src, twig::Format::Markdown, ext).unwrap();
    let doc = Doc::ingest(&mut td, src).unwrap();
    (td, doc)
}

/// The core invariant: `ast_json`'s tree walk and the flat array's tree walk
/// describe the same nodes in the same order. `Doc::ingest` checks kind AND
/// span at every step, so any divergence is an error rather than a silent
/// misattribution of every style rule after it.
#[test]
fn the_two_read_surfaces_agree_across_every_construct() {
    let src = "\
---
title: T
---

# H1

Para with *em*, **strong**, `code`, [link](u), ![img](i).

:::note{.warn}
Directive body.
:::

1) one
2) two

- [ ] todo
- [x] done

> quote

| a | b |
|:--|--:|
| 1 | 2 |

```zig
const x = 1;
```

Ref[^1].

[^1]: Footnote body.
";
    let (_, doc) = parse(src);
    assert_eq!(doc.node(doc.root()).kind, twig::Kind::Doc);
    // If ingest returned at all, the lockstep check passed on every node.
    assert!(doc.nodes.len() > 40, "got {} nodes", doc.nodes.len());
}

/// Footnote definitions are parsed as arena nodes with no parent, outside the
/// document tree, so `ast_json` never emits them. They must still be ingested
/// or footnote bodies silently vanish from the output.
#[test]
fn detached_footnote_definitions_are_ingested_with_their_label() {
    let (_, doc) = parse("Text[^a].\n\n[^a]: The body.\n");

    let fnote = doc
        .nodes
        .iter()
        .find(|n| n.kind == twig::Kind::Footnote)
        .expect("the footnote definition must be ingested despite being detached");

    assert_eq!(
        fnote.str_field("label"),
        Some("a"),
        "the label is in neither projection and must be read off the source span"
    );
    assert!(fnote.parent.is_none(), "it is a detached root");
    assert!(!fnote.children.is_empty(), "its body must come with it");
}

/// Payloads that only `ast_json` carries — the whole reason we do not just use
/// `FlatNode`. If twig ever drops one of these from the JSON, this fails.
#[test]
fn json_only_payloads_survive_ingest() {
    let (_, doc) = parse("---\ntitle: T\n---\n\n```zig\nx\n```\n\n3) three\n\n- [x] done\n");
    let get = |kind: twig::Kind| doc.nodes.iter().find(|n| n.kind == kind).unwrap();

    assert_eq!(get(twig::Kind::Metadata).str_field("lang"), Some("yaml"));
    assert_eq!(get(twig::Kind::CodeBlock).str_field("lang"), Some("zig"));
    assert_eq!(get(twig::Kind::OrderedList).int_field("start"), Some(3));
    assert_eq!(
        get(twig::Kind::TaskListItem).bool_field("checked"),
        Some(true)
    );
}

/// Rules are matched by twig's OWN selector engine, so the config surface and
/// `twig query` cannot drift. This checks the id round-trip that makes that
/// possible: query returns arena ids, and every one maps back to an ingested
/// node.
#[test]
fn selector_matches_map_back_to_ingested_nodes() {
    let src = ":::note{.warn}\nBody.\n:::\n\n:::note\nOther.\n:::\n";
    let (mut td, doc) = parse(src);

    let cfg = Config {
        rules: vec![Rule {
            select: "container[name=note][class~=warn]".into(),
            style: Style {
                hidden: Some(true),
                ..Default::default()
            },
        }],
        ..Default::default()
    };

    let resolved = twig_export::resolve::resolve(&cfg, &doc, &mut td).unwrap();
    assert_eq!(
        resolved.match_counts,
        vec![1],
        "the class predicate must select exactly the .warn note"
    );

    let warn = doc
        .nodes
        .iter()
        .position(|n| n.kind == twig::Kind::Container && n.has_class("warn"))
        .unwrap();
    assert!(resolved.is_hidden(&doc, warn));

    // Hiding must reach the subtree, not just the container the rule matched.
    let body = doc.node(warn).children[0];
    assert!(
        resolved.is_hidden(&doc, body),
        "descendants inherit `hidden`"
    );
}

/// An invalid selector must fail at the rule that owns it, naming it.
#[test]
fn an_invalid_selector_names_its_rule() {
    let (mut td, doc) = parse("# H\n");
    let cfg = Config {
        rules: vec![Rule {
            select: "!!nonsense!!".into(),
            style: Style::default(),
        }],
        ..Default::default()
    };
    let err = twig_export::resolve::resolve(&cfg, &doc, &mut td)
        .unwrap_err()
        .to_string();
    assert!(err.contains("rule #1"), "{err}");
}
