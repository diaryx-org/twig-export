//! Ingest: a Twig document as an owned tree whose nodes carry BOTH every AST
//! payload and the `NodeId` twig's own selector engine speaks.
//!
//! ── Why two sources ────────────────────────────────────────────────────────
//! Neither of twig's read surfaces alone is enough for a renderer.
//!
//! `Document::nodes()` (the flat `FlatNode` array) carries the `NodeId`s that
//! `Document::query()` returns — the ids we need to attach style rules to
//! nodes — but its payload projection is lossy for our purposes: it has
//! `level`/`destination`/`name`/`attrs`, and NOT a code block's `lang`, an
//! ordered list's `numbering`/`start`, a task item's `checked`, or a metadata
//! island's `lang`. A PDF renderer needs all four.
//!
//! `Document::ast_json()` carries every payload (verified: `numbering`,
//! `start`, `tight`, `checked`, `lang`, `alignment`, `colspan`/`rowspan`,
//! `form`, `argument`) and no ids at all.
//!
//! So we walk both and zip them, matching on kind AND span at every step so a
//! divergence is a loud error naming the node rather than every style rule
//! after it being silently misattributed.
//!
//! ── Detached nodes: why the zip is structural, not positional ──────────────
//! The obvious zip — "both are document-order pre-order walks, so pair them by
//! index" — is wrong, and twig tells you so on any document with a footnote.
//!
//! `nodes()` dumps the whole ARENA. `ast_json()` walks the TREE from the `doc`
//! root. Those differ, because a footnote *definition* is parsed into a node
//! with `parent == None` that is not among the root's descendants — it is
//! reachable by id, and by the `footnote_reference` that names it, but not by
//! walking children. On `examples/report.md` the arena holds 116 nodes and the
//! tree holds 113; the three extra are one `footnote` and its `para`/`str`.
//!
//! So the tree half of the zip walks the flat array's own links
//! (`first_child`/`next_sibling`) from the root, which yields exactly the node
//! sequence `ast_json` emits, in the same order, regardless of where the arena
//! happens to place anything. Detached subtrees are then ingested separately
//! ([`ingest_detached`]) so that footnote bodies still render and still match
//! selectors — they just carry `FlatNode`'s thinner payload projection, which
//! for a footnote body (paragraphs and text) loses nothing that matters.
//!
//! ── Why not reimplement the selector engine ────────────────────────────────
//! The config surface is *defined* in terms of twig's CSS-lite selectors
//! (`src/ast/select.zig`). Reimplementing that grammar here would mean two
//! copies drifting apart, and a config file that silently means different
//! things to `twig query` and to us. Zipping the ids across is the cost of
//! having exactly one implementation of the grammar.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value as Json;

/// Our index into [`Doc::nodes`] — a position in the document-order walk.
/// Distinct from twig's `NodeId` (an arena id); [`Doc::by_node_id`] maps
/// between them.
pub type Idx = usize;

/// Twig's arena id for a node, as returned by `query()` / `nodes()`.
///
/// A bare `u32` rather than `twig::NodeId` because twig's two read surfaces
/// spell it differently — `FlatNode.id` is a `NodeId` newtype, `QueryMatch.node_id`
/// is a plain `u32` — and this is the type that has to join them.
pub type NodeId = u32;

/// One node: the JSON payload object, plus tree links and the twig id.
#[derive(Debug, Clone)]
pub struct Node {
    /// The published kind name (`"heading"`, `"container"`, `"str"`, …) — the
    /// same flat vocabulary `twig query` selectors match against.
    pub kind: twig::Kind,
    pub node_id: NodeId,
    pub parent: Option<Idx>,
    pub children: Vec<Idx>,
    /// Byte span into the original source.
    pub span: (usize, usize),
    /// Every non-structural field of the JSON node: `level`, `lang`, `text`,
    /// `numbering`, `start`, `tight`, `checked`, `name`, `form`, `argument`,
    /// `destination`, `alignment`, `head`, `colspan`, `rowspan`, …
    ///
    /// Kept as raw JSON rather than a typed enum on purpose: twig's kind
    /// vocabulary grows (`AST-KINDS.md` is explicit that it is one flat,
    /// extensible union), and a renderer that reads payloads by name degrades
    /// to "unstyled but present" for a kind it has never heard of, instead of
    /// failing to deserialize the whole document.
    pub payload: Json,
    /// `{...}` / HTML attributes in source order. A `None` value is a *bare*
    /// attribute (HTML `disabled`), which round-trips distinctly from `=""`.
    pub attrs: Vec<(String, Option<String>)>,
}

impl Node {
    /// A payload field as a string, or `None` if absent or not a string.
    pub fn str_field(&self, key: &str) -> Option<&str> {
        self.payload.get(key)?.as_str()
    }

    /// A payload field as an integer, or `None` if absent or not a number.
    pub fn int_field(&self, key: &str) -> Option<i64> {
        self.payload.get(key)?.as_i64()
    }

    /// A payload field as a bool, or `None` if absent or not a bool.
    pub fn bool_field(&self, key: &str) -> Option<bool> {
        self.payload.get(key)?.as_bool()
    }

    /// The first value for `key` in [`attrs`](Self::attrs). An outer `None`
    /// means the key is absent; `Some(None)` means present-but-bare.
    pub fn attr(&self, key: &str) -> Option<Option<&str>> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_deref())
    }

    /// Whether `class` contains `word` as a whitespace-separated token — the
    /// same test as a selector's `[class~=word]`.
    pub fn has_class(&self, word: &str) -> bool {
        self.attr("class")
            .flatten()
            .is_some_and(|v| v.split_whitespace().any(|t| t == word))
    }
}

/// A whole ingested document.
pub struct Doc {
    /// Every node in document order. `nodes[0]` is the `doc` root.
    pub nodes: Vec<Node>,
    by_node_id: HashMap<NodeId, Idx>,
    /// The original source bytes, so a renderer can recover raw spans.
    pub source: String,
}

impl Doc {
    pub fn root(&self) -> Idx {
        0
    }

    pub fn node(&self, idx: Idx) -> &Node {
        &self.nodes[idx]
    }

    /// Map one of twig's arena ids (from `query()`) to our index.
    pub fn by_node_id(&self, id: NodeId) -> Option<Idx> {
        self.by_node_id.get(&id).copied()
    }

    /// The document's first `metadata` island, if it has one: `(lang, text)`.
    /// `lang` is the config language the frontmatter was written in, stored
    /// exactly as the fence tag was written — which is what tells [`crate::meta`]
    /// which fig parser to hand the text to.
    pub fn metadata(&self) -> Option<(&str, &str)> {
        self.nodes.iter().find(|n| n.kind == twig::Kind::Metadata).and_then(|n| {
            Some((n.str_field("lang").unwrap_or("yaml"), n.str_field("text")?))
        })
    }

    /// Concatenated descendant text of `idx` — the same projection a
    /// selector's `:contains(...)` matches against.
    pub fn text_of(&self, idx: Idx) -> String {
        let mut out = String::new();
        self.collect_text(idx, &mut out);
        out
    }

    fn collect_text(&self, idx: Idx, out: &mut String) {
        let n = self.node(idx);
        if let Some(t) = n.str_field("text")
            && matches!(n.kind.as_str(), "str" | "verbatim" | "code" | "text_leaf")
        {
            out.push_str(t);
        }
        for &c in &n.children {
            self.collect_text(c, out);
        }
    }

    /// Build a `Doc` from a parsed twig document.
    ///
    /// Zips `ast_json()` (payloads) against `nodes()` (ids) — see the module
    /// docs for why both are needed and why the correspondence is checked
    /// rather than trusted.
    pub fn ingest(td: &mut twig::Document, source: &str) -> Result<Self> {
        let json_bytes = td.ast_json().context("twig ast_json failed")?;
        let json: Json =
            serde_json::from_slice(&json_bytes).context("twig emitted unparseable AST JSON")?;

        let flat = td.nodes().context("twig nodes() failed")?;
        let by_id: HashMap<NodeId, &twig::FlatNode> =
            flat.iter().map(|n| (n.id.0, n)).collect();

        let root = flat.first().context("twig returned an empty node array")?;
        if root.parent.is_some() {
            bail!("twig's first flat node is not a root");
        }
        let root_id = root.id.0;

        // The tree half of the zip: the root's subtree in document order,
        // following the flat array's own links rather than its layout.
        let mut order = Vec::with_capacity(flat.len());
        preorder(root_id, &by_id, &mut order);

        let mut nodes = Vec::with_capacity(flat.len());
        let mut cursor = 0usize;
        build(&json, None, &order, &by_id, &mut cursor, &mut nodes)?;

        if cursor != order.len() {
            bail!(
                "AST walk mismatch: ast_json yielded {cursor} nodes, the flat tree walk \
                 yielded {}. The two twig read surfaces have diverged; see src/doc.rs.",
                order.len()
            );
        }

        // Everything the tree walk did not reach: footnote definitions and
        // anything else twig parks outside the document tree.
        for n in &flat {
            if n.parent.is_none() && n.id.0 != root_id {
                ingest_detached(n.id.0, None, &by_id, source, &mut nodes);
            }
        }

        let by_node_id = nodes
            .iter()
            .enumerate()
            .map(|(i, n): (Idx, &Node)| (n.node_id, i))
            .collect();

        Ok(Doc {
            nodes,
            by_node_id,
            source: source.to_string(),
        })
    }
}

/// Document-order ids of `id`'s subtree, via the flat array's tree links.
fn preorder(id: NodeId, by_id: &HashMap<NodeId, &twig::FlatNode>, out: &mut Vec<NodeId>) {
    let Some(n) = by_id.get(&id) else { return };
    out.push(id);
    let mut child = n.first_child.map(|c| c.0);
    while let Some(c) = child {
        preorder(c, by_id, out);
        child = by_id.get(&c).and_then(|n| n.next_sibling).map(|s| s.0);
    }
}

/// Ingest a subtree that the JSON walk never saw, from `FlatNode` alone.
///
/// The payload is reconstructed from the flat projection, so these nodes carry
/// `level`/`text`/`destination`/`name`/`head`/`alignment` and not the fields
/// only `ast_json` emits (`lang`, `numbering`, `checked`, …). For the one
/// construct that lands here today — a footnote body, which is paragraphs and
/// inline text — that is the full set of fields anything reads.
fn ingest_detached(
    id: NodeId,
    parent: Option<Idx>,
    by_id: &HashMap<NodeId, &twig::FlatNode>,
    source: &str,
    out: &mut Vec<Node>,
) -> Option<Idx> {
    let fl = by_id.get(&id)?;
    let idx = out.len();
    let span = (fl.span.start, fl.span.end);

    let mut payload = flat_payload(fl);
    // A footnote definition's LABEL is in neither projection: `ast_json` never
    // emits the node, and `FlatNode` has no field for it. Recover it from the
    // source the way twig itself says to — every node carries a byte span into
    // the real input, and slicing it is the authoritative read.
    if fl.kind == twig::Kind::Footnote
        && let Some(label) = footnote_label(source, span)
        && let Some(map) = payload.as_object_mut()
    {
        map.insert("label".into(), Json::String(label));
    }

    out.push(Node {
        kind: fl.kind.clone(),
        node_id: id,
        parent,
        children: Vec::new(),
        span,
        payload,
        attrs: fl.attrs.clone(),
    });

    let mut kids = Vec::new();
    let mut child = fl.first_child.map(|c| c.0);
    while let Some(c) = child {
        if let Some(k) = ingest_detached(c, Some(idx), by_id, source, out) {
            kids.push(k);
        }
        child = by_id.get(&c).and_then(|n| n.next_sibling).map(|s| s.0);
    }
    out[idx].children = kids;
    Some(idx)
}

/// The label out of a `[^label]: …` definition, read off the source span.
fn footnote_label(source: &str, span: (usize, usize)) -> Option<String> {
    let text = source.get(span.0..span.1)?;
    let rest = text.trim_start().strip_prefix("[^")?;
    let end = rest.find(']')?;
    Some(rest[..end].to_string())
}

/// A `FlatNode`'s payload fields as a JSON object, so [`Node::str_field`] and
/// friends work uniformly across both ingest paths.
fn flat_payload(fl: &twig::FlatNode) -> Json {
    let mut map = serde_json::Map::new();
    map.insert("kind".into(), Json::String(fl.kind.to_string()));
    if let Some(l) = fl.level {
        map.insert("level".into(), Json::from(l));
    }
    if let Some(t) = &fl.text {
        map.insert("text".into(), Json::String(t.clone()));
    }
    if let Some(d) = &fl.destination {
        map.insert("destination".into(), Json::String(d.clone()));
    }
    if let Some(n) = &fl.name {
        map.insert("name".into(), Json::String(n.clone()));
    }
    if let Some(h) = fl.head {
        map.insert("head".into(), Json::Bool(h));
    }
    Json::Object(map)
}

/// Recursive half of [`Doc::ingest`]: consume one JSON node and its subtree,
/// advancing `cursor` through the flat array in lockstep.
fn build(
    json: &Json,
    parent: Option<Idx>,
    order: &[NodeId],
    by_id: &HashMap<NodeId, &twig::FlatNode>,
    cursor: &mut usize,
    out: &mut Vec<Node>,
) -> Result<Idx> {
    let obj = json
        .as_object()
        .ok_or_else(|| anyhow!("AST JSON node #{cursor} is not an object"))?;

    let kind_string = obj
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| anyhow!("AST JSON node #{cursor} has no `kind`"))?
        .to_string();

    let span = obj
        .get("span")
        .and_then(Json::as_array)
        .and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)))
        .ok_or_else(|| anyhow!("AST JSON node #{cursor} ({kind_string}) has no `span`"))?;

    let fl = order
        .get(*cursor)
        .and_then(|id| by_id.get(id))
        .ok_or_else(|| {
            anyhow!("the flat tree walk ran out at #{cursor} while ast_json still had `{kind_string}`")
        })?;

    let kind = twig::Kind::from(kind_string.as_str());

    // The correspondence check the module docs promise. Kind AND span must
    // agree; if twig ever reorders one walk without the other, this names the
    // exact node instead of silently mis-attributing every style rule after it.
    if fl.kind != kind || fl.span.start != span.0 || fl.span.end != span.1 {
        bail!(
            "AST walk mismatch at #{cursor}: ast_json says `{kind_string}` {span:?}, \
             the flat tree walk says `{}` {:?}. See src/doc.rs.",
            fl.kind,
            (fl.span.start, fl.span.end)
        );
    }

    let idx = out.len();
    *cursor += 1;

    // Everything that is not structure is payload.
    let mut payload = obj.clone();
    payload.remove("children");
    payload.remove("attrs");
    payload.remove("span");
    payload.remove("content_span");

    out.push(Node {
        kind: kind,
        node_id: fl.id.0,
        parent,
        children: Vec::new(),
        span,
        payload: Json::Object(payload),
        attrs: fl.attrs.clone(),
    });

    if let Some(kids) = obj.get("children").and_then(Json::as_array) {
        let mut child_idxs = Vec::with_capacity(kids.len());
        for k in kids {
            child_idxs.push(build(k, Some(idx), order, by_id, cursor, out)?);
        }
        out[idx].children = child_idxs;
    }

    Ok(idx)
}
