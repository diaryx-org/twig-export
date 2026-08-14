//! Resolution: config rules + document → a style per node.
//!
//! Each rule's selector goes to twig's own engine (`Document::query`), which
//! returns the matching `NodeId`s; [`crate::doc::Doc`] maps those to our
//! indices. Rules are applied in source order, each overlaying the last — see
//! [`crate::config`] on why precedence is source order rather than
//! specificity.
//!
//! Inheritance is deliberately NOT modelled here. A style attaches to the node
//! its rule matched and to nothing else. Typst already inherits text
//! properties through content nesting, so a `font` set on a `container`
//! genuinely does reach the paragraphs inside it — but that is Typst's
//! inheritance doing the work at layout time, not a cascade we compute. Trying
//! to precompute inherited values here would mean resolving relative units
//! (`1.2em` of what?) against a font size we do not know until layout, which
//! is precisely the job we are delegating to a typesetter.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::doc::{Doc, Idx};
use crate::style::Style;

/// Every node's resolved style, plus what the resolution did — the latter
/// exists so `--explain` can answer "why is this heading not red?", which is
/// otherwise the single most annoying question a style system can pose.
#[derive(Debug)]
pub struct Resolved {
    styles: HashMap<Idx, Style>,
    /// Per rule, in config order: how many nodes it matched.
    pub match_counts: Vec<usize>,
    /// Preamble fragments contributed by rules that matched at least once,
    /// in config order, deduplicated.
    pub preamble: Vec<String>,
}

impl Resolved {
    pub fn style(&self, idx: Idx) -> Option<&Style> {
        self.styles.get(&idx)
    }

    /// Whether the node (or any ancestor) is dropped from the output.
    ///
    /// Checked against ancestors too: hiding a `:::private` container has to
    /// hide its contents, and the rule matched the container, not the
    /// paragraphs inside it.
    pub fn is_hidden(&self, doc: &Doc, idx: Idx) -> bool {
        let mut cur = Some(idx);
        while let Some(i) = cur {
            if self.styles.get(&i).and_then(|s| s.hidden) == Some(true) {
                return true;
            }
            cur = doc.node(i).parent;
        }
        false
    }
}

/// Apply `cfg`'s rules to `doc`.
///
/// `td` is the same twig document `doc` was ingested from — its selector
/// engine is what actually does the matching.
pub fn resolve(cfg: &Config, doc: &Doc, td: &mut twig::Document) -> Result<Resolved> {
    let mut styles: HashMap<Idx, Style> = HashMap::new();
    let mut match_counts = Vec::with_capacity(cfg.rules.len());
    let mut preamble: Vec<String> = Vec::new();

    for (i, rule) in cfg.rules.iter().enumerate() {
        let matches = td.query(&rule.select).with_context(|| {
            format!(
                "rule #{} has an invalid selector {:?} \
                 (grammar: twig's src/ast/select.zig)",
                i + 1,
                rule.select
            )
        })?;

        let mut n = 0usize;
        for m in &matches {
            let Some(idx) = doc.by_node_id(m.node_id) else {
                // A queried id with no ingested counterpart would mean the two
                // walks disagree, which `Doc::ingest` already proved they do
                // not. Skip rather than panic.
                continue;
            };
            styles.entry(idx).or_default().overlay(&rule.style);
            n += 1;
        }
        match_counts.push(n);

        if n > 0
            && let Some(p) = rule.style.raw.as_ref().and_then(|r| r.preamble.as_ref())
            && !preamble.iter().any(|e| e == p)
        {
            preamble.push(p.clone());
        }
    }

    Ok(Resolved {
        styles,
        match_counts,
        preamble,
    })
}
