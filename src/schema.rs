//! The config schema: what each field expects, checked before deserialization.
//!
//! ── Why a schema rather than checks after deserialization ──────────────────
//! The config file IS the product here — "which directives and metadata drive
//! which styling" is the whole feature — so the quality of a config error is
//! not a side concern. Serde alone reports a bad value as a type error with no
//! path, and a misspelled property as `unknown field`, neither of which tells
//! you what to type instead.
//!
//! [`fig_schema`] validates the `fig::Value` tree *before* it becomes typed
//! structs, which buys three things serde cannot:
//!
//! - **A path.** Every finding names `rule[2].box.fill`, not "a string".
//! - **Near-miss suggestions.** `weigth` → "did you mean `weight`?", and
//!   `semibol` → "did you mean `semibold`?", via `fig_schema`'s edit-distance
//!   search over the vocabulary.
//! - **Warn versus reject.** Some vocabularies are genuinely closed (`weight`
//!   has seven members and Typst will not grow an eighth this week) and some
//!   are open (`paper` names, font families). A closed vocabulary rejects; an
//!   open one warns and passes the value through. Serde can only hard-fail,
//!   which is why `paper` and `font` are plain strings to it.
//!
//! ── What is deliberately NOT used ─────────────────────────────────────────
//! `fig_schema::Presentation` (icons, tints) carries display hints for an
//! editor UI over a config file. twig-export is a batch renderer with no such
//! surface, so every rule here uses `Presentation::default()`. If a config
//! editor ever wants one, the hints belong in these same rules.

use anyhow::{Result, bail};
use fig::Value;
use fig_schema::{
    FieldRule, FieldType, PathPat, Presentation, Schema, Seg, Term, Validate, Validation,
    validate_enum,
};

/// A constraint on one config field.
///
/// `fig_schema` is generic over this type by design — it never learns a
/// consumer's vocabulary — so the four shapes twig-export actually needs live
/// here.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// A controlled vocabulary. `closed` decides reject versus warn.
    Vocab { terms: Vec<Term>, closed: bool },
    /// A number followed by a unit Typst accepts.
    Length,
    /// `#rgb` / `#rrggbb` / `#rrggbbaa`, or a colour name.
    Color,
    /// A twig selector.
    Selector,
}

impl Validate for Constraint {
    fn validate(&self, value: &Value) -> Validation {
        match self {
            Constraint::Vocab { terms, closed } => validate_enum(terms, *closed, value),
            Constraint::Length => scalar_check(value, check_length),
            Constraint::Color => scalar_check(value, check_color),
            Constraint::Selector => scalar_check(value, check_selector),
        }
    }
}

/// Apply a string check to a scalar, leaving non-strings to serde's own type
/// error (which is perfectly clear for `size = true`).
fn scalar_check(value: &Value, f: fn(&str) -> Result<(), String>) -> Validation {
    let Value::Str(s) = value else {
        return Validation::Ok;
    };
    match f(s) {
        Ok(()) => Validation::Ok,
        Err(msg) => Validation::Reject(fig_schema::Issue::custom(s.clone(), msg)),
    }
}

/// Units Typst accepts wherever it accepts a length. `%` and `fr` are here
/// because `width = "80%"` means something perfectly sensible.
const UNITS: &[&str] = &["pt", "mm", "cm", "in", "em", "%", "fr"];

fn check_length(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v == "0" {
        return Ok(());
    }
    let Some(unit) = UNITS.iter().find(|u| v.ends_with(**u)) else {
        return Err(format!(
            "“{v}” has no unit — expected a number followed by {}",
            UNITS.join(", ")
        ));
    };
    let num = &v[..v.len() - unit.len()];
    if num.is_empty() || num.parse::<f64>().is_err() {
        return Err(format!("“{v}” is not a number followed by {unit}"));
    }
    Ok(())
}

fn check_color(v: &str) -> Result<(), String> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        if !matches!(hex.len(), 3 | 4 | 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("“{v}” is not a #rgb / #rrggbb / #rrggbbaa colour"));
        }
        return Ok(());
    }
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Ok(());
    }
    Err(format!("“{v}” is neither a hex colour nor a colour name"))
}

/// A cheap structural check. Deliberately NOT a parser: twig owns the selector
/// grammar, and a second implementation is exactly the drift this crate exists
/// to avoid. The authoritative check happens when [`crate::resolve`] hands the
/// string to twig's engine; this only catches the typos worth catching before
/// a document is opened.
fn check_selector(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        return Err("selector is empty".into());
    }
    for (open, close) in [('[', ']'), ('(', ')')] {
        if v.chars().filter(|&c| c == open).count() != v.chars().filter(|&c| c == close).count() {
            return Err(format!("“{v}” has unbalanced {open} {close}"));
        }
    }
    Ok(())
}

fn vocab(values: &[&str], closed: bool) -> Constraint {
    Constraint::Vocab {
        terms: values.iter().map(|v| Term::value(*v)).collect(),
        closed,
    }
}

fn rule(segs: Vec<Seg2>, ty: FieldType, constraint: Option<Constraint>) -> FieldRule<Constraint> {
    FieldRule {
        at: PathPat(segs.into_iter().map(Seg2::into_pat).collect()),
        ty: Some(ty),
        constraint,
        present: Presentation::default(),
    }
}

/// A tiny builder so the schema table below reads as paths rather than as
/// nested enum constructors.
enum Seg2 {
    K(&'static str),
    Each,
}

impl Seg2 {
    fn into_pat(self) -> fig_schema::SegPat {
        match self {
            Seg2::K(k) => fig_schema::SegPat::Key(k.into()),
            Seg2::Each => fig_schema::SegPat::EachItem,
        }
    }
}

/// Every property name valid inside a `[[rule]]`, for key-spelling
/// suggestions. Kept in sync with [`crate::style::Style`] by
/// `tests/schema.rs`, which fails if a field is added there and not here.
pub const STYLE_KEYS: &[&str] = &[
    "select",
    "font",
    "size",
    "weight",
    "italic",
    "color",
    "tracking",
    "smallcaps",
    "underline",
    "strike",
    "leading",
    "space-before",
    "space-after",
    "indent",
    "margin-left",
    "margin-right",
    "align",
    "break-before",
    "break-after",
    "box",
    "hidden",
    "unwrap",
    "numbering",
    "raw",
];

/// Every key valid inside `[document]`.
pub const DOCUMENT_KEYS: &[&str] = &[
    "paper",
    "margin",
    "font",
    "size",
    "leading",
    "align",
    "lang",
    "title",
    "author",
    "page-numbering",
    "toc-depth",
    "raw-preamble",
    "title-page",
    "from-metadata",
];

/// Every key valid inside a `[rule.box]`.
pub const BOX_KEYS: &[&str] = &["fill", "stroke", "stroke-width", "padding", "radius", "width"];

/// Every key valid inside a `[rule.raw]`.
pub const RAW_KEYS: &[&str] = &["before", "after", "wrap", "preamble"];

const WEIGHTS: &[&str] = &[
    "thin",
    "light",
    "regular",
    "medium",
    "semibold",
    "bold",
    "black",
];
const ALIGNS: &[&str] = &["left", "center", "right", "justify"];
const BREAKS: &[&str] = &["none", "page", "column"];
/// Open, not closed: Typst knows more paper names than are worth listing, and
/// adds them faster than this table would be updated. An unrecognised one
/// warns and is passed straight through.
const PAPERS: &[&str] = &[
    "a3",
    "a4",
    "a5",
    "a6",
    "us-letter",
    "us-legal",
    "us-executive",
    "iso-b4",
    "iso-b5",
    "presentation-16-9",
    "presentation-4-3",
];

/// The config schema.
pub fn schema() -> Schema<Constraint> {
    use FieldType::{Bool, Int, Str};
    use Seg2::{Each, K};

    Schema::new(vec![
        // ── [document] ────────────────────────────────────────────────────
        rule(vec![K("document"), K("paper")], Str, Some(vocab(PAPERS, false))),
        rule(vec![K("document"), K("margin")], Str, Some(Constraint::Length)),
        rule(vec![K("document"), K("size")], Str, Some(Constraint::Length)),
        rule(vec![K("document"), K("leading")], Str, Some(Constraint::Length)),
        rule(vec![K("document"), K("align")], Str, Some(vocab(ALIGNS, true))),
        rule(vec![K("document"), K("toc-depth")], Int, None),
        // ── [[rule]] ──────────────────────────────────────────────────────
        rule(vec![K("rule"), Each, K("select")], Str, Some(Constraint::Selector)),
        rule(vec![K("rule"), Each, K("weight")], Str, Some(vocab(WEIGHTS, true))),
        rule(vec![K("rule"), Each, K("align")], Str, Some(vocab(ALIGNS, true))),
        rule(vec![K("rule"), Each, K("break-before")], Str, Some(vocab(BREAKS, true))),
        rule(vec![K("rule"), Each, K("break-after")], Str, Some(vocab(BREAKS, true))),
        rule(vec![K("rule"), Each, K("color")], Str, Some(Constraint::Color)),
        rule(vec![K("rule"), Each, K("hidden")], Bool, None),
        rule(vec![K("rule"), Each, K("unwrap")], Bool, None),
        rule(vec![K("rule"), Each, K("italic")], Bool, None),
        rule(vec![K("rule"), Each, K("size")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("tracking")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("leading")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("space-before")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("space-after")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("indent")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("margin-left")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("margin-right")], Str, Some(Constraint::Length)),
        // ── [rule.box] ────────────────────────────────────────────────────
        rule(vec![K("rule"), Each, K("box"), K("fill")], Str, Some(Constraint::Color)),
        rule(vec![K("rule"), Each, K("box"), K("stroke")], Str, Some(Constraint::Color)),
        rule(vec![K("rule"), Each, K("box"), K("stroke-width")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("box"), K("padding")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("box"), K("radius")], Str, Some(Constraint::Length)),
        rule(vec![K("rule"), Each, K("box"), K("width")], Str, Some(Constraint::Length)),
    ])
}

/// One schema finding, with the path that produced it.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Dotted path, e.g. `rule[2].box.fill`.
    pub path: String,
    pub validation: Validation,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.validation.issue() {
            Some(issue) => write!(f, "{}: {issue}", self.path),
            None => write!(f, "{}: ok", self.path),
        }
    }
}

/// Validate a parsed config value tree.
///
/// Returns every finding, warnings included. The caller decides what is fatal
/// — [`crate::config::Config::load`] rejects on `Validation::Reject` and
/// surfaces warnings.
pub fn check(value: &Value) -> Vec<Finding> {
    let schema = schema();
    let mut out = Vec::new();
    walk(value, &mut Vec::new(), &schema, &mut out);
    out
}

fn walk(value: &Value, path: &mut Vec<Seg>, schema: &Schema<Constraint>, out: &mut Vec<Finding>) {
    // Check keys before descending, so a misspelled property is reported at
    // the key rather than silently having no rule and passing.
    if let Value::Map(entries) = value
        && let Some(allowed) = keys_allowed_at(path)
    {
        for (k, _) in entries {
            if let Value::Str(key) = k {
                let v = validate_enum(
                    &allowed.iter().map(|k| Term::value(*k)).collect::<Vec<_>>(),
                    true,
                    &Value::Str(key.clone()),
                );
                if !v.is_ok() {
                    out.push(Finding {
                        path: render_path(path, Some(key)),
                        validation: v,
                    });
                }
            }
        }
    }

    if let Some(r) = schema.rule_for(path) {
        let v = r.validate(value);
        if !v.is_ok() {
            out.push(Finding {
                path: render_path(path, None),
                validation: v,
            });
        }
    }

    match value {
        Value::Map(entries) => {
            for (k, v) in entries {
                let Value::Str(key) = k else { continue };
                path.push(Seg::Key(key.clone()));
                walk(v, path, schema, out);
                path.pop();
            }
        }
        Value::Seq(items) => {
            for (i, item) in items.iter().enumerate() {
                path.push(Seg::Index(i));
                walk(item, path, schema, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// The closed key vocabulary at a given path, if there is one.
///
/// `document.from-metadata` deliberately has none: its keys are the *document
/// fields* being bound, and its values are whatever the author's frontmatter
/// calls them, so neither side is a fixed vocabulary this crate could close.
fn keys_allowed_at(path: &[Seg]) -> Option<&'static [&'static str]> {
    let keys: Vec<&str> = path
        .iter()
        .map(|s| match s {
            Seg::Key(k) => k.as_str(),
            Seg::Index(_) => "*",
        })
        .collect();
    match keys.as_slice() {
        ["document"] => Some(DOCUMENT_KEYS),
        ["rule", "*"] => Some(STYLE_KEYS),
        ["rule", "*", "box"] => Some(BOX_KEYS),
        ["rule", "*", "raw"] => Some(RAW_KEYS),
        _ => None,
    }
}

/// `rule[2].box.fill` — indices in brackets, keys dotted.
fn render_path(path: &[Seg], extra_key: Option<&str>) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Index(i) => out.push_str(&format!("[{i}]")),
        }
    }
    if let Some(k) = extra_key {
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(k);
    }
    if out.is_empty() {
        "<root>".into()
    } else {
        out
    }
}

/// Fail if any finding is a rejection, listing every one.
pub fn reject_on_errors(findings: &[Finding]) -> Result<()> {
    let errors: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.validation.is_reject())
        .collect();
    if errors.is_empty() {
        return Ok(());
    }
    let mut msg = String::from("invalid config:");
    for e in errors {
        msg.push_str(&format!("\n  {e}"));
    }
    bail!(msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(toml: &str) -> Vec<Finding> {
        let doc = fig::Document::parse(toml.as_bytes(), fig::Format::Toml).unwrap();
        check(&doc.to_value().unwrap())
    }

    #[test]
    fn a_misspelled_property_is_caught_at_its_path_with_a_suggestion() {
        let f = findings("[[rule]]\nselect = \"heading\"\nweigth = \"bold\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "rule[0].weigth");
        assert_eq!(
            f[0].validation.issue().unwrap().suggestion.as_deref(),
            Some("weight"),
            "a near miss must be offered"
        );
    }

    #[test]
    fn a_misspelled_vocabulary_value_suggests_the_real_one() {
        let f = findings("[[rule]]\nselect = \"heading\"\nweight = \"semibol\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "rule[0].weight");
        assert!(f[0].validation.is_reject(), "weight is a closed vocabulary");
        assert_eq!(
            f[0].validation.issue().unwrap().suggestion.as_deref(),
            Some("semibold")
        );
    }

    /// An open vocabulary must warn and pass the value through — Typst knows
    /// more paper names than this table lists.
    #[test]
    fn an_unknown_paper_warns_rather_than_rejecting() {
        let f = findings("[document]\npaper = \"a9\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(!f[0].validation.is_reject(), "paper is an open vocabulary");
        assert!(!f[0].validation.is_ok());
    }

    #[test]
    fn a_bad_length_is_reported_with_its_path() {
        let f = findings("[[rule]]\nselect = \"heading\"\n\n[rule.box]\npadding = \"3\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "rule[0].box.padding");
        assert!(f[0].validation.is_reject());
    }

    #[test]
    fn from_metadata_keys_are_not_a_closed_vocabulary() {
        let f = findings("[document.from-metadata]\npaper = \"pdf-paper\"\ntitle = \"title\"\n");
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn the_shipped_example_config_is_clean() {
        let text =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/report.toml"))
                .unwrap();
        let f = findings(&text);
        assert!(f.is_empty(), "the example config must validate cleanly: {f:?}");
    }
}
