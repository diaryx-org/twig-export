//! Keeps the schema's key vocabularies honest against the types they describe.
//!
//! The schema lists property names as string constants so it can offer
//! near-miss suggestions ("`weigth` — did you mean `weight`?"). That list is a
//! second copy of the field names in [`twig_export::style`], and a second copy
//! is a drift bug waiting to happen: add a field to `Style` and forget the
//! constant, and the new property silently reports as an unknown key.
//!
//! These tests close that loop by serializing a fully-populated value and
//! comparing the key sets, so the compiler-adjacent guarantee is "adding a
//! field fails a test" rather than "someone remembers".

use twig_export::schema::{BOX_KEYS, DOCUMENT_KEYS, RAW_KEYS, STYLE_KEYS};
use twig_export::style::{Align, Break, BoxStyle, RawTypst, Style, Weight};

/// Serialize a value and collect its top-level keys.
fn keys_of<T: serde::Serialize>(value: &T) -> Vec<String> {
    let json = serde_json::to_value(value).unwrap();
    json.as_object()
        .expect("expected a struct")
        .keys()
        .cloned()
        .collect()
}

/// Every field set, so serde emits every key (the fields are `Option` and
/// would otherwise be skipped).
fn populated_style() -> Style {
    Style {
        font: Some("f".into()),
        size: Some("1pt".into()),
        weight: Some(Weight::Bold),
        italic: Some(true),
        color: Some("#000".into()),
        tracking: Some("1pt".into()),
        smallcaps: Some(true),
        underline: Some(true),
        strike: Some(true),
        leading: Some("1pt".into()),
        space_before: Some("1pt".into()),
        space_after: Some("1pt".into()),
        indent: Some("1pt".into()),
        margin_left: Some("1pt".into()),
        margin_right: Some("1pt".into()),
        align: Some(Align::Left),
        break_before: Some(Break::Page),
        break_after: Some(Break::Page),
        box_: Some(BoxStyle {
            fill: Some("#000".into()),
            stroke: Some("#000".into()),
            stroke_width: Some("1pt".into()),
            padding: Some("1pt".into()),
            radius: Some("1pt".into()),
            width: Some("1pt".into()),
        }),
        hidden: Some(true),
        unwrap: Some(true),
        numbering: Some("1.".into()),
        raw: Some(RawTypst {
            before: Some("a".into()),
            after: Some("b".into()),
            wrap: Some("c".into()),
            preamble: Some("d".into()),
        }),
    }
}

#[test]
fn style_keys_match_the_style_struct() {
    let mut actual = keys_of(&populated_style());
    // `select` lives on `Rule`, not `Style`, but is a valid key inside a
    // `[[rule]]` table because `Style` is flattened into it.
    actual.push("select".into());
    actual.sort();

    let mut expected: Vec<String> = STYLE_KEYS.iter().map(|s| s.to_string()).collect();
    expected.sort();

    assert_eq!(
        actual, expected,
        "schema::STYLE_KEYS is out of sync with style::Style"
    );
}

#[test]
fn box_keys_match_the_box_struct() {
    let mut actual = keys_of(&populated_style().box_.unwrap());
    actual.sort();
    let mut expected: Vec<String> = BOX_KEYS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(actual, expected, "schema::BOX_KEYS is out of sync");
}

#[test]
fn raw_keys_match_the_raw_struct() {
    let mut actual = keys_of(&populated_style().raw.unwrap());
    actual.sort();
    let mut expected: Vec<String> = RAW_KEYS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(actual, expected, "schema::RAW_KEYS is out of sync");
}

#[test]
fn document_keys_match_the_document_struct() {
    let mut actual = keys_of(&twig_export::config::Document::default());
    actual.sort();
    let mut expected: Vec<String> = DOCUMENT_KEYS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(actual, expected, "schema::DOCUMENT_KEYS is out of sync");
}
