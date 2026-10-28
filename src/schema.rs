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
    FieldRule, FieldType, PathPat, Schema, Seg, Term, Validate, Validation, validate_enum,
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
    /// A raw-Typst field that `crate::typst_emit::interpolate` runs
    /// `{{...}}` placeholders through. `node_scoped` distinguishes
    /// `rule.raw.before`/`after`/`wrap` (resolved per matched node —
    /// `attr`/`field`/`text`) from `document.raw-preamble`/`title-page`
    /// (resolved against the document config — `doc`, whose `NAME` half is
    /// also checked, against [`crate::config::DOC_FIELD_NAMES`]).
    Interpolation { node_scoped: bool },
}

impl Validate for Constraint {
    fn validate(&self, value: &Value) -> Validation {
        match self {
            Constraint::Vocab { terms, closed } => validate_enum(terms, *closed, value),
            Constraint::Length => scalar_check(value, check_length),
            Constraint::Color => scalar_check(value, check_color),
            Constraint::Selector => scalar_check(value, check_selector),
            Constraint::Interpolation { node_scoped } => check_interpolation(*node_scoped, value),
        }
    }
}

/// The `{{...}}` placeholder keywords a raw-Typst field may use — see
/// `crate::typst_emit::interpolate_node`/`interpolate_doc`.
const NODE_PLACEHOLDER_KEYWORDS: &[&str] = &["attr", "field", "text"];
const DOC_PLACEHOLDER_KEYWORDS: &[&str] = &["doc"];

/// Validate every `{{...}}` placeholder in a raw-Typst string: the keyword
/// (`attr`/`field`/`text`, or `doc`) against a closed vocabulary — so
/// `{{atr.count}}` is rejected with "did you mean `attr`?", the same near-miss
/// machinery `weight`/`align` already get — and, for `doc`, the name half
/// against [`crate::config::DOC_FIELD_NAMES`] too, since that one IS a fixed,
/// enumerable set (unlike an attribute or payload field name, which depends
/// on whatever kind a rule's selector happens to match).
///
/// `fig_schema::Validate` returns one `Validation` per field, so a string
/// with several placeholders reports whichever is worst — same as
/// `validate_enum` already does for a sequence's elements.
fn check_interpolation(node_scoped: bool, value: &Value) -> Validation {
    let Value::Str(s) = value else {
        return Validation::Ok;
    };
    let keywords = if node_scoped {
        NODE_PLACEHOLDER_KEYWORDS
    } else {
        DOC_PLACEHOLDER_KEYWORDS
    };
    let keyword_terms: Vec<Term> = keywords.iter().map(|k| Term::value(*k)).collect();
    let doc_field_terms: Vec<Term> = crate::config::DOC_FIELD_NAMES
        .iter()
        .map(|n| Term::value(*n))
        .collect();

    let mut worst = Validation::Ok;
    for token in crate::typst_emit::placeholder_tokens(s) {
        let (keyword, name) = match token.split_once('.') {
            Some((k, n)) => (k, Some(n)),
            None => (token, None),
        };

        let result = validate_enum(&keyword_terms, true, &Value::Str(keyword.into()));
        let unknown_keyword = result.is_reject();
        worst = worse(worst, result);

        // A shape/name check only makes sense once the keyword itself is
        // recognised — an unknown keyword already explains itself.
        if unknown_keyword {
            continue;
        }
        match (keyword, name) {
            ("text", Some(_)) => {
                worst = worse(
                    worst,
                    Validation::Reject(fig_schema::Issue::custom(
                        token,
                        "`text` takes no name — write `{{text}}`, not `{{text.…}}`",
                    )),
                );
            }
            (_, None) if keyword != "text" => {
                worst = worse(
                    worst,
                    Validation::Reject(fig_schema::Issue::custom(
                        token,
                        format!("`{{{{{keyword}}}}}` needs a name — `{{{{{keyword}.NAME}}}}`"),
                    )),
                );
            }
            ("doc", Some(name)) => {
                worst = worse(
                    worst,
                    validate_enum(&doc_field_terms, true, &Value::Str(name.into())),
                );
            }
            _ => {}
        }
    }
    worst
}

/// The more severe of two validation results — `fig_schema` keeps this
/// ranking private, so a multi-placeholder string tracks it by hand the same
/// way `validate_enum` already does across a sequence's elements.
fn worse(a: Validation, b: Validation) -> Validation {
    fn rank(v: &Validation) -> u8 {
        match v {
            Validation::Ok => 0,
            Validation::Warn(_) => 1,
            Validation::Reject(_) => 2,
        }
    }
    if rank(&b) > rank(&a) { b } else { a }
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
    // The builder rather than a struct expression: `FieldRule` is
    // `#[non_exhaustive]` since fig-schema 0.2, so a field it gains later
    // arrives as its default here instead of as a compile error.
    FieldRule::new(PathPat(segs.into_iter().map(Seg2::into_pat).collect()))
        .ty(ty)
        .constraint_opt(constraint)
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
    "overflow-wrap",
    "template",
    "layout",
    "page-width",
    "page-height",
    "dark",
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
pub const BOX_KEYS: &[&str] = &[
    "fill",
    "stroke",
    "stroke-width",
    "padding",
    "radius",
    "width",
];

/// Every key valid inside a `[rule.raw]`.
pub const RAW_KEYS: &[&str] = &["before", "after", "wrap", "preamble"];

const WEIGHTS: &[&str] = &[
    "thin", "light", "regular", "medium", "semibold", "bold", "black",
];
const ALIGNS: &[&str] = &["left", "center", "right", "justify"];

/// What `document.overflow-wrap` may be: Typst's `par.overflow-wrap`.
const OVERFLOW_WRAPS: &[&str] = &["normal", "break-word"];

/// What `document.layout` may be.
const LAYOUTS: &[&str] = &["paged", "reflow"];
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
        rule(
            vec![K("document"), K("paper")],
            Str,
            Some(vocab(PAPERS, false)),
        ),
        rule(
            vec![K("document"), K("margin")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("document"), K("size")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("document"), K("leading")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("document"), K("align")],
            Str,
            Some(vocab(ALIGNS, true)),
        ),
        rule(
            vec![K("document"), K("overflow-wrap")],
            Str,
            Some(vocab(OVERFLOW_WRAPS, true)),
        ),
        rule(
            vec![K("document"), K("template")],
            Str,
            Some(vocab(crate::templates::NAMES, true)),
        ),
        rule(
            vec![K("document"), K("layout")],
            Str,
            Some(vocab(LAYOUTS, true)),
        ),
        rule(
            vec![K("document"), K("page-width")],
            Str,
            Some(Constraint::Length),
        ),
        rule(vec![K("document"), K("dark")], Bool, None),
        rule(vec![K("document"), K("toc-depth")], Int, None),
        rule(
            vec![K("document"), K("raw-preamble")],
            Str,
            Some(Constraint::Interpolation { node_scoped: false }),
        ),
        rule(
            vec![K("document"), K("title-page")],
            Str,
            Some(Constraint::Interpolation { node_scoped: false }),
        ),
        // ── [[rule]] ──────────────────────────────────────────────────────
        rule(
            vec![K("rule"), Each, K("select")],
            Str,
            Some(Constraint::Selector),
        ),
        rule(
            vec![K("rule"), Each, K("weight")],
            Str,
            Some(vocab(WEIGHTS, true)),
        ),
        rule(
            vec![K("rule"), Each, K("align")],
            Str,
            Some(vocab(ALIGNS, true)),
        ),
        rule(
            vec![K("rule"), Each, K("break-before")],
            Str,
            Some(vocab(BREAKS, true)),
        ),
        rule(
            vec![K("rule"), Each, K("break-after")],
            Str,
            Some(vocab(BREAKS, true)),
        ),
        rule(
            vec![K("rule"), Each, K("color")],
            Str,
            Some(Constraint::Color),
        ),
        rule(vec![K("rule"), Each, K("hidden")], Bool, None),
        rule(vec![K("rule"), Each, K("unwrap")], Bool, None),
        rule(vec![K("rule"), Each, K("italic")], Bool, None),
        rule(
            vec![K("rule"), Each, K("size")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("tracking")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("leading")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("space-before")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("space-after")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("indent")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("margin-left")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("margin-right")],
            Str,
            Some(Constraint::Length),
        ),
        // ── [rule.raw] ────────────────────────────────────────────────────
        // Not `before`/`after`/`wrap`'s sibling `preamble`: that field is
        // never interpolated (it is emitted once globally, not per matched
        // node — see `crate::typst_emit::emit`), so it has no placeholder
        // grammar to validate.
        rule(
            vec![K("rule"), Each, K("raw"), K("before")],
            Str,
            Some(Constraint::Interpolation { node_scoped: true }),
        ),
        rule(
            vec![K("rule"), Each, K("raw"), K("after")],
            Str,
            Some(Constraint::Interpolation { node_scoped: true }),
        ),
        rule(
            vec![K("rule"), Each, K("raw"), K("wrap")],
            Str,
            Some(Constraint::Interpolation { node_scoped: true }),
        ),
        // ── [rule.box] ────────────────────────────────────────────────────
        rule(
            vec![K("rule"), Each, K("box"), K("fill")],
            Str,
            Some(Constraint::Color),
        ),
        rule(
            vec![K("rule"), Each, K("box"), K("stroke")],
            Str,
            Some(Constraint::Color),
        ),
        rule(
            vec![K("rule"), Each, K("box"), K("stroke-width")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("box"), K("padding")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("box"), K("radius")],
            Str,
            Some(Constraint::Length),
        ),
        rule(
            vec![K("rule"), Each, K("box"), K("width")],
            Str,
            Some(Constraint::Length),
        ),
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
    if out.is_empty() { "<root>".into() } else { out }
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
        let doc = fig::Document::parse(toml.as_bytes(), fig_toml::format().unwrap()).unwrap();
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
        assert!(
            f.is_empty(),
            "the example config must validate cleanly: {f:?}"
        );
    }

    /// The other shipped example is the one that actually uses `{{...}}`
    /// placeholders — `report.toml` alone would not exercise this check at
    /// all.
    #[test]
    fn the_folklore_example_config_is_clean() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/folklore-book.toml"
        ))
        .unwrap();
        let f = findings(&text);
        assert!(
            f.is_empty(),
            "the example config must validate cleanly: {f:?}"
        );
    }

    #[test]
    fn a_misspelled_placeholder_keyword_is_rejected_with_a_suggestion() {
        let f = findings("[[rule]]\nselect = \"heading\"\nraw.before = \"#f({{atr.count}})\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].path, "rule[0].raw.before");
        assert!(f[0].validation.is_reject());
        assert_eq!(
            f[0].validation.issue().unwrap().suggestion.as_deref(),
            Some("attr")
        );
    }

    #[test]
    fn attr_field_and_text_placeholders_are_all_accepted() {
        let f = findings(
            "[[rule]]\nselect = \"heading\"\n\
             raw.before = \"#f({{attr.count}}, {{field.level}}, {{text}})\"\n",
        );
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn text_with_a_name_is_rejected() {
        let f = findings("[[rule]]\nselect = \"heading\"\nraw.before = \"{{text.foo}}\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
    }

    #[test]
    fn a_keyword_without_a_name_is_rejected() {
        let f = findings("[[rule]]\nselect = \"heading\"\nraw.before = \"{{attr}}\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
    }

    /// `doc` is closed to `attr`/`field`/`text` too: those resolve per matched
    /// node, and a document-level raw field (`raw-preamble`/`title-page`) has
    /// no matched node to resolve them against.
    #[test]
    fn attr_is_rejected_in_a_document_level_raw_field() {
        let f = findings("[document]\nraw-preamble = \"{{attr.count}}\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
        assert_eq!(f[0].validation.issue().unwrap().suggestion.as_deref(), None);
    }

    #[test]
    fn doc_is_rejected_in_a_node_scoped_raw_field() {
        let f = findings("[[rule]]\nselect = \"heading\"\nraw.wrap = \"{{doc.title}}\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
    }

    /// The `doc.NAME` half is also a closed, enumerable vocabulary — unlike
    /// `attr`/`field`, whose names depend on whatever kind a selector
    /// happens to match, `Document`'s fields are fixed.
    #[test]
    fn an_unknown_doc_field_name_is_rejected_with_a_suggestion() {
        let f = findings("[document]\ntitle-page = \"{{doc.titel}}\"\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
        assert_eq!(
            f[0].validation.issue().unwrap().suggestion.as_deref(),
            Some("title")
        );
    }

    /// `raw.preamble` is never interpolated (see `crate::typst_emit::emit`),
    /// so it carries no placeholder grammar to validate — `{{...}}` there is
    /// just literal text as far as the schema is concerned.
    #[test]
    fn raw_preamble_is_not_checked_for_placeholders() {
        let f = findings("[[rule]]\nselect = \"heading\"\nraw.preamble = \"{{nonsense}}\"\n");
        assert!(f.is_empty(), "{f:?}");
    }

    /// The worst placeholder in a field wins, same as `validate_enum` already
    /// does across a sequence's elements.
    #[test]
    fn the_worst_of_several_placeholders_in_one_field_is_reported() {
        let f = findings(
            "[[rule]]\nselect = \"heading\"\nraw.before = \"{{attr.count}} {{atr.other}}\"\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].validation.is_reject());
    }
}
