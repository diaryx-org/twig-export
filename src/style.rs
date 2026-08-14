//! The neutral style vocabulary.
//!
//! Every property here is a *typesetting* concept, not a Typst one: a future
//! CSS-paged-media or handrolled backend could honour the same config file.
//! Anything the vocabulary genuinely cannot express goes through
//! [`Style::raw`], which is explicitly and visibly backend-specific — see
//! [`RawTypst`].
//!
//! ── Why every field is an `Option` ─────────────────────────────────────────
//! A rule sets the properties it mentions and says nothing about the rest, so
//! rules compose by overlay ([`Style::overlay`]) rather than replacement. That
//! is what makes the two-rule idiom work:
//!
//! ```text
//! container[name=note]            -> { box.fill = "#eef", box.stroke = "#88a" }
//! container[name=note][class~=warn] -> { box.fill = "#fee" }
//! ```
//!
//! A `.warn` note keeps the base note's stroke and padding and overrides only
//! the fill. With non-optional fields the second rule would have to restate
//! every property of the first.
//!
//! ── Why lengths are strings ────────────────────────────────────────────────
//! `"2.5cm"`, `"11pt"`, `"0.65em"`, `"80%"` — a length is parsed and validated
//! by [`crate::config`] at load time (so a typo is a config error with a line
//! reference, not a mystery in the output) but carried as the authored string.
//! Round-tripping the authored spelling keeps `--dump-config` honest and means
//! a relative unit like `em` stays relative rather than being resolved against
//! a font size we do not know yet.

use serde::{Deserialize, Serialize};

/// A resolved set of style properties. `None` means "this rule says nothing
/// about that property", which is what lets rules overlay.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct Style {
    // ── Text ──────────────────────────────────────────────────────────────
    /// Font family name, e.g. `"Libertinus Serif"`.
    pub font: Option<String>,
    /// Font size as a length: `"11pt"`, `"1.2em"`.
    pub size: Option<String>,
    pub weight: Option<Weight>,
    pub italic: Option<bool>,
    /// Text colour as `#rgb` / `#rrggbb`.
    pub color: Option<String>,
    /// Letter spacing.
    pub tracking: Option<String>,
    pub smallcaps: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,

    // ── Block layout ──────────────────────────────────────────────────────
    /// Baseline-to-baseline spacing within a block.
    pub leading: Option<String>,
    pub space_before: Option<String>,
    pub space_after: Option<String>,
    /// First-line indent.
    pub indent: Option<String>,
    /// Indent applied to the whole block (both margins for a quote, say).
    pub margin_left: Option<String>,
    pub margin_right: Option<String>,
    pub align: Option<Align>,
    pub break_before: Option<Break>,
    pub break_after: Option<Break>,

    // ── Decoration ────────────────────────────────────────────────────────
    /// Draw the block inside a frame. This is the property that makes
    /// `:::note` admonitions work without a raw escape hatch.
    ///
    /// Spelled `box` in a config file; `box_` here only because `box` is a
    /// reserved word in Rust.
    #[serde(rename = "box")]
    pub box_: Option<BoxStyle>,

    // ── Structural ────────────────────────────────────────────────────────
    /// Drop the node and its subtree from the output entirely. The point of
    /// this is audience filtering: `container[name=vis][class~=private]` →
    /// `hidden = true` is the PDF analogue of `twig filter --drop`.
    pub hidden: Option<bool>,
    /// Render the node's children but not the node's own decoration — the
    /// styling analogue of `twig filter --unwrap`.
    pub unwrap: Option<bool>,
    /// Numbering pattern for headings and ordered lists, in Typst's numbering
    /// mini-language (`"1.1"`, `"a)"`, `"I."`). Neutral enough to keep: it is
    /// the same convention docutils and CSS counters express.
    pub numbering: Option<String>,

    // ── Escape hatch ──────────────────────────────────────────────────────
    /// Backend-specific passthrough. Deliberately its own field with a
    /// backend name in it, so that reading a config makes it obvious which
    /// parts are portable and which are not.
    pub raw: Option<RawTypst>,
}

/// Verbatim Typst injected around a node.
///
/// This exists because a neutral vocabulary that covers *everything* is just
/// Typst with worse ergonomics — but a config where the escape hatch is
/// invisible is a config you cannot port. Naming the field `raw.typst` makes
/// the coupling explicit at the point of use.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct RawTypst {
    /// Emitted immediately before the node.
    pub before: Option<String>,
    /// Emitted immediately after the node.
    pub after: Option<String>,
    /// A Typst function applied to the node's rendered content. The content
    /// is passed as a trailing content block, so `"note-box"` emits
    /// `#note-box[ …content… ]` and `"note-box.with(variant: \"warn\")"`
    /// works too.
    pub wrap: Option<String>,
    /// Typst emitted once in the preamble when this rule matches anything —
    /// where a `#let note-box(body) = { … }` definition goes.
    pub preamble: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Weight {
    Thin,
    Light,
    Regular,
    Medium,
    Semibold,
    Bold,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Break {
    None,
    Page,
    Column,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default, rename_all = "kebab-case")]
pub struct BoxStyle {
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: Option<String>,
    pub padding: Option<String>,
    pub radius: Option<String>,
    pub width: Option<String>,
}

impl BoxStyle {
    fn overlay(&mut self, other: &BoxStyle) {
        overlay_opt(&mut self.fill, &other.fill);
        overlay_opt(&mut self.stroke, &other.stroke);
        overlay_opt(&mut self.stroke_width, &other.stroke_width);
        overlay_opt(&mut self.padding, &other.padding);
        overlay_opt(&mut self.radius, &other.radius);
        overlay_opt(&mut self.width, &other.width);
    }
}

impl RawTypst {
    fn overlay(&mut self, other: &RawTypst) {
        // `before`/`after`/`preamble` CONCATENATE rather than replace: two
        // rules that both want to inject something around the same node
        // (a page break AND a marker, say) should both get their injection.
        // `wrap` replaces, because two wrappers would nest ambiguously and
        // the later, more specific rule is the one that meant it.
        concat_opt(&mut self.before, &other.before);
        concat_opt(&mut self.after, &other.after);
        concat_opt(&mut self.preamble, &other.preamble);
        overlay_opt(&mut self.wrap, &other.wrap);
    }
}

impl Style {
    /// Apply `other` on top of `self`: every property `other` sets wins, every
    /// property it leaves `None` is inherited. This is the cascade, and it is
    /// intentionally the *only* precedence mechanism — see [`crate::config`]
    /// on why ordering is source order rather than CSS-style specificity.
    pub fn overlay(&mut self, other: &Style) {
        overlay_opt(&mut self.font, &other.font);
        overlay_opt(&mut self.size, &other.size);
        overlay_opt(&mut self.weight, &other.weight);
        overlay_opt(&mut self.italic, &other.italic);
        overlay_opt(&mut self.color, &other.color);
        overlay_opt(&mut self.tracking, &other.tracking);
        overlay_opt(&mut self.smallcaps, &other.smallcaps);
        overlay_opt(&mut self.underline, &other.underline);
        overlay_opt(&mut self.strike, &other.strike);

        overlay_opt(&mut self.leading, &other.leading);
        overlay_opt(&mut self.space_before, &other.space_before);
        overlay_opt(&mut self.space_after, &other.space_after);
        overlay_opt(&mut self.indent, &other.indent);
        overlay_opt(&mut self.margin_left, &other.margin_left);
        overlay_opt(&mut self.margin_right, &other.margin_right);
        overlay_opt(&mut self.align, &other.align);
        overlay_opt(&mut self.break_before, &other.break_before);
        overlay_opt(&mut self.break_after, &other.break_after);

        overlay_opt(&mut self.hidden, &other.hidden);
        overlay_opt(&mut self.unwrap, &other.unwrap);
        overlay_opt(&mut self.numbering, &other.numbering);

        match (&mut self.box_, &other.box_) {
            (Some(a), Some(b)) => a.overlay(b),
            (slot @ None, Some(b)) => *slot = Some(b.clone()),
            _ => {}
        }
        match (&mut self.raw, &other.raw) {
            (Some(a), Some(b)) => a.overlay(b),
            (slot @ None, Some(b)) => *slot = Some(b.clone()),
            _ => {}
        }
    }

    /// Whether this style would change anything at all. An all-`None` style
    /// lets the emitter skip a wrapper entirely and keep the `.typ` readable.
    pub fn is_empty(&self) -> bool {
        *self == Style::default()
    }
}

fn overlay_opt<T: Clone>(slot: &mut Option<T>, other: &Option<T>) {
    if let Some(v) = other {
        *slot = Some(v.clone());
    }
}

fn concat_opt(slot: &mut Option<String>, other: &Option<String>) {
    if let Some(v) = other {
        match slot {
            Some(existing) => {
                existing.push('\n');
                existing.push_str(v);
            }
            None => *slot = Some(v.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_keeps_unmentioned_properties() {
        let mut base = Style {
            box_: Some(BoxStyle {
                fill: Some("#eef".into()),
                stroke: Some("#88a".into()),
                padding: Some("0.8em".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let variant = Style {
            box_: Some(BoxStyle {
                fill: Some("#fee".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        base.overlay(&variant);

        let b = base.box_.unwrap();
        assert_eq!(b.fill.as_deref(), Some("#fee"), "the variant's fill wins");
        assert_eq!(b.stroke.as_deref(), Some("#88a"), "the base's stroke survives");
        assert_eq!(b.padding.as_deref(), Some("0.8em"));
    }

    #[test]
    fn raw_injections_concatenate_but_wrap_replaces() {
        let mut a = Style {
            raw: Some(RawTypst {
                before: Some("#pagebreak()".into()),
                wrap: Some("outer".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        a.overlay(&Style {
            raw: Some(RawTypst {
                before: Some("#counter(page).update(1)".into()),
                wrap: Some("inner".into()),
                ..Default::default()
            }),
            ..Default::default()
        });

        let raw = a.raw.unwrap();
        assert_eq!(
            raw.before.as_deref(),
            Some("#pagebreak()\n#counter(page).update(1)")
        );
        assert_eq!(raw.wrap.as_deref(), Some("inner"));
    }
}
