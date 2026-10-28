//! The Typst backend: document + resolved styles → `.typ` source.
//!
//! ── Why `.typ` is the real target and PDF is a step on top ─────────────────
//! Everything interesting this crate does happens here, and it is all
//! text-to-text: an AST goes in, Typst markup comes out. That makes the whole
//! mapping testable with golden files (`tests/golden/`) the way twig tests its
//! own corpora, rather than by diffing binary PDFs. It also means a rendering
//! bug is something you can *read*. PDF is then `typst::compile` over this
//! output ([`crate::pdf`]), and a `.typ` file is a useful artifact in its own
//! right for anyone who wants to take the result further by hand.
//!
//! ── The neutral vocabulary lands here, and only here ───────────────────────
//! [`crate::style::Style`] is backend-agnostic by construction; this module is
//! the single place that knows a `Style::box_` becomes `#block(fill: …)`. A
//! second backend (CSS paged media, say) is a sibling of this file and needs
//! no changes anywhere else. That is the whole reason the vocabulary is
//! neutral instead of being Typst show-rules with extra steps.
//!
//! ── The words as content ─────────────────────────────────────────────────
//! [`emit`] writes the author's text into the `.typ` as escaped markup.
//! [`emit_words`] writes the same structure, styles and escape hatches, but
//! leaves a hole wherever the author's text would go — `#_twig.at(3);` —
//! and hands back the [`Word`]s, each with the byte of the source it came
//! from. The whole document is then one template function of the words,
//! which [`crate::pdf`] calls with each word as Typst content spanned to its
//! place in the source. A glyph on the page then leads back to the
//! Markdown byte it was set from, which an editor drawing on the page needs;
//! no text is escaped; and typing inside a paragraph leaves the generated
//! source as it was, so Typst's cache keeps every other block.

use std::cell::RefCell;
use std::fmt::Write as _;

use crate::config::{Config, Document, weight_str};
use crate::doc::{Doc, Idx};
use crate::resolve::Resolved;
use crate::style::{Align, Break, Style};

/// The template function [`emit_words`] defines, and its one parameter, the
/// words. Leading underscores keep both out of the way of a config's own
/// raw Typst.
pub const TEMPLATE: &str = "_twig-page";
pub const WORDS: &str = "_twig";

/// A run of the author's text, set as Typst content rather than markup: the
/// text, the byte of the source it starts at, and whether it is code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub at: usize,
    pub code: Option<Code>,
}

/// Code, set as Typst's raw text: its language, whether it is a block, and
/// the byte of the source each of its lines (`text` split at `\n`) starts
/// at, so that a glyph on any line leads back to its own byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    pub lang: Option<String>,
    pub block: bool,
    pub lines: Vec<usize>,
}

/// What the emitter reads beside the tree: the resolved styles, and, when
/// it writes holes, the words it has written them for.
struct Cx<'a> {
    res: &'a Resolved,
    words: Option<&'a RefCell<Vec<Word>>>,
}

impl<'a> Cx<'a> {
    fn is_hidden(&self, doc: &Doc, idx: Idx) -> bool {
        self.res.is_hidden(doc, idx)
    }

    fn style(&self, idx: Idx) -> Option<&'a Style> {
        self.res.style(idx)
    }

    /// Node `idx`'s text `s`: escaped markup, or a hole for it. A hole ends
    /// in `;`, so whatever follows it is markup again, never more of the
    /// expression.
    fn text(&self, doc: &Doc, idx: Idx, s: &str) -> String {
        match self.words {
            None => escape(s),
            Some(_) if s.is_empty() => String::new(),
            Some(words) => {
                let mut words = words.borrow_mut();
                let mut out = String::new();
                for (text, at) in runs(doc, idx, s) {
                    words.push(Word {
                        text: text.to_string(),
                        at,
                        code: None,
                    });
                    let _ = write!(out, "#{WORDS}.at({});", words.len() - 1);
                }
                out
            }
        }
    }

    /// Node `idx`'s code `text`: a `#raw` call, or a hole for it. A call,
    /// not a fence: a fence's content is column-sensitive, and
    /// `wrap_block` indents whatever it wraps, so a styled code block inside
    /// a `#block[…]` would silently gain leading whitespace on every line.
    /// A string argument is immune to the surrounding indentation, and it
    /// sidesteps fence-length arithmetic against backticks in the content.
    fn code(&self, doc: &Doc, idx: Idx, text: &str, lang: Option<&str>, block: bool) -> String {
        let Some(words) = self.words else {
            let mut args = Vec::new();
            if block {
                args.push("block: true".to_string());
            }
            if let Some(lang) = lang {
                args.push(format!("lang: {}", quote(lang)));
            }
            args.push(quote(text));
            return format!("#raw({})", args.join(", "));
        };
        let (from, to) = doc.node(idx).span;
        let source = doc.source.get(from..to).unwrap_or_default();
        let mut cursor = 0;
        let lines: Vec<usize> = text
            .split('\n')
            .map(|line| {
                let start = source[cursor..].find(line).map_or(cursor, |p| cursor + p);
                cursor = start + line.len();
                from + start
            })
            .collect();
        let mut words = words.borrow_mut();
        words.push(Word {
            text: text.to_string(),
            at: lines.first().copied().unwrap_or(from),
            code: Some(Code {
                lang: lang.map(str::to_string),
                block,
                lines,
            }),
        });
        format!("#{WORDS}.at({});", words.len() - 1)
    }

    /// The same, writing text as text: for a string Typst reads as a string
    /// (an image's `alt`), where content cannot go.
    fn plain(&self) -> Cx<'a> {
        Cx {
            res: self.res,
            words: None,
        }
    }
}

/// Node `idx`'s text `s` as runs that each stand whole in the source, and
/// the byte each starts at: one run, unless the source spells the text with
/// a backslash escape (`\\"` for `"`), after which the text is a new run.
/// The text is looked for from the node's span on, then from a few bytes
/// before it, since a parser's span can start a character early (twig's,
/// after an unmatched `[`).
fn runs<'s>(doc: &Doc, idx: Idx, s: &'s str) -> Vec<(&'s str, usize)> {
    let source = &doc.source;
    let (from, to) = doc.node(idx).span;
    let hi = (to + 4).min(source.len());
    let mut lo = from.saturating_sub(4);
    while !source.is_char_boundary(lo) {
        lo -= 1;
    }
    let first = s.chars().next().map(String::from).unwrap_or_default();
    let found = |pat: &str| {
        let after = source.get(from..hi)?.find(pat).map(|p| from + p);
        after.or_else(|| source.get(lo..hi)?.find(pat).map(|p| lo + p))
    };
    let Some(mut at) = found(s).or_else(|| found(&first)) else {
        return vec![(s, from)];
    };
    let mut runs = Vec::new();
    let mut start = 0;
    let mut run_at = at;
    for (i, c) in s.char_indices() {
        let mut here = source[at..].chars();
        match here.next() {
            Some(d) if d == c => at += c.len_utf8(),
            // The escape is the source's, not the text's: the text goes on
            // a byte later, as a new run.
            Some('\\') if here.next() == Some(c) => {
                if i > start {
                    runs.push((&s[start..i], run_at));
                }
                start = i;
                run_at = at + 1;
                at += 1 + c.len_utf8();
            }
            // Spelled some other way (an entity, a reference): the rest is
            // one run, from here.
            _ => break,
        }
    }
    runs.push((&s[start..], run_at));
    runs
}

/// The words a document names its face with, which a template sets in its
/// own faces: Diaryx's `font:` words.
const FACE_WORDS: &[&str] = &["serif", "sans-serif", "monospace", "handwritten"];

/// Render a document to Typst source.
pub fn emit(doc: &Doc, cfg: &Config, resolved: &Resolved) -> String {
    let cx = Cx {
        res: resolved,
        words: None,
    };
    let mut out = template_source(cfg);
    preamble(&mut out, cfg, resolved);
    body(&mut out, doc, &cx);
    out
}

/// The import of the template the config names, to stand above everything
/// else in the source; nothing without one. A name the config gives that
/// is not a template has been refused by then (`render`).
fn template_source(cfg: &Config) -> String {
    let template = cfg.document.template.as_deref();
    match template.and_then(crate::templates::spec) {
        Some(spec) => format!("#import {}: twig-template\n", quote(&spec)),
        None => String::new(),
    }
}

/// Render a document to Typst source with a hole for each run of the
/// author's text, and the words that fill them: the source defines one
/// function, [`TEMPLATE`], which takes the words as an array of content
/// and is the whole document.
pub fn emit_words(doc: &Doc, cfg: &Config, resolved: &Resolved) -> (String, Vec<Word>) {
    let words = RefCell::new(Vec::new());
    let cx = Cx {
        res: resolved,
        words: Some(&words),
    };
    let mut out = template_source(cfg);
    let _ = writeln!(out, "#let {TEMPLATE}({WORDS}) = [");
    preamble(&mut out, cfg, resolved);
    body(&mut out, doc, &cx);
    out.push_str("]\n");
    (out, words.into_inner())
}

fn body(out: &mut String, doc: &Doc, cx: &Cx) {
    for &child in &doc.node(doc.root()).children {
        emit_block(out, doc, cx, child);
    }
}

fn preamble(out: &mut String, cfg: &Config, resolved: &Resolved) {
    let d = &cfg.document;
    out.push_str("// Generated by twig-export. Edits here are not round-tripped.\n");

    // A template sets the page, the face and the size, given them as its
    // arguments; what else the document sets comes after it, and so wins.
    let templated = d
        .template
        .as_deref()
        .and_then(crate::templates::spec)
        .is_some();
    if templated {
        let mut args = vec![format!(
            "layout: {}",
            quote(d.layout.as_deref().unwrap_or("paged"))
        )];
        if let Some(p) = &d.paper {
            args.push(format!("paper: {}", quote(p)));
        }
        if let Some(w) = &d.page_width {
            args.push(format!("width: {w}"));
        }
        if let Some(h) = &d.page_height {
            args.push(format!("height: {h}"));
        }
        if let Some(s) = &d.size {
            args.push(format!("size: {s}"));
        }
        if let Some(f) = d.font.as_deref().filter(|f| FACE_WORDS.contains(f)) {
            args.push(format!("face: {}", quote(f)));
        }
        if let Some(dark) = d.dark {
            args.push(format!("dark: {dark}"));
        }
        if !d.meta.is_empty() {
            let fields: Vec<String> = d
                .meta
                .iter()
                .map(|(k, v)| format!("{}: {}", quote(k), typst_value(v)))
                .collect();
            args.push(format!("meta: ({})", fields.join(", ")));
        }
        let _ = writeln!(out, "#show: twig-template.with({})", args.join(", "));
    }

    let mut page_args: Vec<String> = Vec::new();
    if let Some(p) = d.paper.as_ref().filter(|_| !templated) {
        page_args.push(format!("paper: {}", quote(p)));
    }
    if let Some(m) = d.margin.as_ref().filter(|_| !templated) {
        page_args.push(format!("margin: {m}"));
    }
    if let Some(n) = &d.page_numbering {
        page_args.push(format!("numbering: {}", quote(n)));
    }
    if !page_args.is_empty() {
        let _ = writeln!(out, "#set page({})", page_args.join(", "));
    }

    let mut text_args: Vec<String> = Vec::new();
    if let Some(f) = d.font.as_ref().filter(|_| !templated) {
        text_args.push(format!("font: {}", quote(f)));
    }
    if let Some(s) = d.size.as_ref().filter(|_| !templated) {
        text_args.push(format!("size: {s}"));
    }
    if let Some(l) = &d.lang {
        text_args.push(format!("lang: {}", quote(l)));
    }
    if !text_args.is_empty() {
        let _ = writeln!(out, "#set text({})", text_args.join(", "));
    }

    let mut par_args: Vec<String> = Vec::new();
    if let Some(l) = &d.leading {
        par_args.push(format!("leading: {l}"));
    }
    if let Some(a) = d.align {
        par_args.push(format!("justify: {}", a == Align::Justify));
    }
    if let Some(w) = &d.overflow_wrap {
        par_args.push(format!("overflow-wrap: {}", quote(w)));
    }
    if !par_args.is_empty() {
        let _ = writeln!(out, "#set par({})", par_args.join(", "));
    }

    if let (Some(title), Some(author)) = (&d.title, &d.author) {
        let _ = writeln!(
            out,
            "#set document(title: {}, author: {})",
            quote(title),
            quote(author)
        );
    } else if let Some(title) = &d.title {
        let _ = writeln!(out, "#set document(title: {})", quote(title));
    }

    if let Some(raw) = &d.raw_preamble {
        out.push_str(&interpolate_doc(raw, d));
        out.push('\n');
    }
    for frag in &resolved.preamble {
        out.push_str(frag);
        out.push('\n');
    }

    out.push('\n');

    // Not gated on `d.title.is_some()` here — the default template's own
    // `#context if document.title != none [...]` does that check in Typst,
    // against whatever `#set document(...)` above actually set. Whether
    // anything is drawn, and how, is entirely up to what this field holds;
    // see `Document::title_page`.
    if let Some(title_page) = &d.title_page {
        out.push_str(&interpolate_doc(title_page, d));
        out.push_str("\n\n");
    }

    if let Some(depth) = d.toc_depth {
        let _ = writeln!(out, "#outline(depth: {depth})\n");
    }
}

/// Emit one block-level node, applying its style.
fn emit_block(out: &mut String, doc: &Doc, cx: &Cx, idx: Idx) {
    if cx.is_hidden(doc, idx) {
        return;
    }
    let empty = Style::default();
    let style = cx.style(idx).unwrap_or(&empty);

    if let Some(raw) = &style.raw
        && let Some(before) = &raw.before
    {
        out.push_str(&interpolate_node(before, doc, idx));
        out.push('\n');
    }
    if style.break_before == Some(Break::Page) {
        out.push_str("#pagebreak()\n");
    } else if style.break_before == Some(Break::Column) {
        out.push_str("#colbreak()\n");
    }

    let mut inner = String::new();
    render_block_content(&mut inner, doc, cx, idx, style);

    let wrapped = wrap_block(&inner, style, doc, idx);
    out.push_str(&wrapped);
    if !wrapped.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');

    if style.break_after == Some(Break::Page) {
        out.push_str("#pagebreak()\n");
    } else if style.break_after == Some(Break::Column) {
        out.push_str("#colbreak()\n");
    }
    if let Some(raw) = &style.raw
        && let Some(after) = &raw.after
    {
        out.push_str(&interpolate_node(after, doc, idx));
        out.push('\n');
    }
}

/// The node's own markup, before any style wrapper.
fn render_block_content(out: &mut String, doc: &Doc, cx: &Cx, idx: Idx, style: &Style) {
    let n = doc.node(idx);

    // `unwrap` drops the node's own decoration and keeps its contents — the
    // styling analogue of `twig filter --unwrap`.
    if style.unwrap == Some(true) {
        for &c in &n.children {
            emit_block(out, doc, cx, c);
        }
        return;
    }

    match n.kind.as_str() {
        // A data island never renders into the body (AST-KINDS.md). Its keys
        // reach the output through `document.from-metadata`, not as text.
        "metadata" => {}

        "heading" => {
            let level = n.int_field("level").unwrap_or(1).clamp(1, 6);
            let _ = write!(out, "{} ", "=".repeat(level as usize));
            emit_inlines(out, doc, cx, &n.children);
        }

        "para" => emit_inlines(out, doc, cx, &n.children),

        "thematic_break" => out.push_str("#line(length: 100%)"),

        "block_quote" => {
            out.push_str("#quote(block: true)[\n");
            let mut body = String::new();
            for &c in &n.children {
                emit_block(&mut body, doc, cx, c);
            }
            out.push_str(&indent(&body));
            out.push(']');
        }

        "code_block" => {
            let text = n.str_field("text").unwrap_or_default();
            let lang = n.str_field("lang").filter(|l| !l.is_empty());
            out.push_str(&cx.code(doc, idx, text, lang, true));
        }

        // Verbatim output aimed at a target format. Emitted only when it was
        // aimed at us; anything else is silently dropped, which is what a
        // `raw_block` means.
        "raw_block" => {
            if matches!(n.str_field("format"), Some("typst") | Some("pdf")) {
                out.push_str(n.str_field("text").unwrap_or_default());
            }
        }

        "bullet_list" => {
            for &c in &n.children {
                let mut item = String::new();
                render_list_item_body(&mut item, doc, cx, c);
                let _ = writeln!(out, "- {}", indent_continuation(&item));
            }
        }

        "ordered_list" => {
            if let Some(start) = n.int_field("start")
                && start != 1
            {
                let _ = writeln!(out, "#set enum(start: {start})");
            }
            if let Some(num) = numbering_pattern(n.str_field("numbering")) {
                let _ = writeln!(out, "#set enum(numbering: {})", quote(num));
            }
            for &c in &n.children {
                let mut item = String::new();
                render_list_item_body(&mut item, doc, cx, c);
                let _ = writeln!(out, "+ {}", indent_continuation(&item));
            }
        }

        "task_list" => {
            for &c in &n.children {
                let checked = doc.node(c).bool_field("checked").unwrap_or(false);
                let mut item = String::new();
                render_list_item_body(&mut item, doc, cx, c);
                let _ = writeln!(
                    out,
                    "- {} {}",
                    checkbox(checked),
                    indent_continuation(&item)
                );
            }
        }

        "definition_list" => {
            for &c in &n.children {
                let dli = doc.node(c);
                let mut term = String::new();
                let mut def = String::new();
                for &g in &dli.children {
                    let gn = doc.node(g);
                    match gn.kind.as_str() {
                        "term" => emit_inlines(&mut term, doc, cx, &gn.children),
                        // A `definition`'s children are inline when the source
                        // was a one-liner and blocks when it was not; twig
                        // emits `str` directly in the former case.
                        _ => render_mixed(&mut def, doc, cx, &gn.children),
                    }
                }
                let _ = writeln!(out, "/ {}: {}", term.trim(), indent_continuation(&def));
            }
        }

        "table" => render_table(out, doc, cx, idx),

        // A run of lines whose breaks ARE the content (verse, addresses).
        // Reflowing would destroy it, so each line gets a hard break.
        "line_block" => {
            for (i, &c) in n.children.iter().enumerate() {
                let line = doc.node(c);
                let depth = line.int_field("indent").unwrap_or(0);
                if i > 0 {
                    out.push_str(" \\\n");
                }
                if depth > 0 {
                    let _ = write!(out, "#h({}em)", depth * 2);
                }
                emit_inlines(out, doc, cx, &line.children);
            }
        }

        "footnote" | "reference" => {
            // Definitions are pulled in at their reference site; emitting them
            // again here would duplicate them.
        }

        "section" | "doc" | "container" => {
            for &c in &n.children {
                emit_block(out, doc, cx, c);
            }
        }

        // An unknown kind renders its children rather than vanishing. Twig's
        // kind vocabulary is explicitly extensible (AST-KINDS.md), so
        // "unstyled but present" is the right failure mode for a kind this
        // backend has not learned yet.
        _ => {
            if n.children.is_empty() {
                emit_inline(out, doc, cx, idx);
            } else {
                for &c in &n.children {
                    emit_block(out, doc, cx, c);
                }
            }
        }
    }
}

/// A list item's body: a tight item is one paragraph inline, a loose item can
/// hold arbitrary blocks.
fn render_list_item_body(out: &mut String, doc: &Doc, cx: &Cx, idx: Idx) {
    if cx.is_hidden(doc, idx) {
        return;
    }
    let n = doc.node(idx);
    let only_para = n.children.len() == 1 && doc.node(n.children[0]).kind == twig::Kind::Para;
    if only_para {
        emit_inlines(out, doc, cx, &doc.node(n.children[0]).children);
    } else {
        for &c in &n.children {
            emit_block(out, doc, cx, c);
        }
    }
}

fn render_table(out: &mut String, doc: &Doc, cx: &Cx, idx: Idx) {
    let n = doc.node(idx);
    let rows: Vec<Idx> = n
        .children
        .iter()
        .copied()
        .filter(|&c| doc.node(c).kind == twig::Kind::Row)
        .collect();
    let cols = rows
        .iter()
        .map(|&r| doc.node(r).children.len())
        .max()
        .unwrap_or(1);

    let aligns: Vec<&str> = rows
        .first()
        .map(|&r| {
            doc.node(r)
                .children
                .iter()
                .map(|&c| match doc.node(c).str_field("alignment") {
                    Some("center") => "center",
                    Some("right") => "right",
                    _ => "left",
                })
                .collect()
        })
        .unwrap_or_default();

    let _ = write!(out, "#table(\n  columns: {cols},\n");
    if !aligns.is_empty() {
        let _ = writeln!(out, "  align: ({},),", aligns.join(", "));
    }

    for &r in &rows {
        let row = doc.node(r);
        let is_head = row.bool_field("head").unwrap_or(false);
        let mut cells: Vec<String> = Vec::new();
        for &c in &row.children {
            let cell = doc.node(c);
            let mut body = String::new();
            emit_inlines(&mut body, doc, cx, &cell.children);
            let colspan = cell.int_field("colspan").unwrap_or(1);
            let rowspan = cell.int_field("rowspan").unwrap_or(1);
            if colspan > 1 || rowspan > 1 {
                cells.push(format!(
                    "table.cell(colspan: {colspan}, rowspan: {rowspan})[{}]",
                    body.trim()
                ));
            } else {
                cells.push(format!("[{}]", body.trim()));
            }
        }
        if is_head {
            let _ = writeln!(out, "  table.header({}),", cells.join(", "));
        } else {
            let _ = writeln!(out, "  {},", cells.join(", "));
        }
    }

    if let Some(&cap) = n
        .children
        .iter()
        .find(|&&c| doc.node(c).kind == twig::Kind::Caption)
    {
        let capnode = doc.node(cap);
        if !capnode.children.is_empty() {
            let mut body = String::new();
            emit_inlines(&mut body, doc, cx, &capnode.children);
            let _ = writeln!(out, "  // caption");
            let _ = write!(out, ")\n#[\n#set align(center)\n#emph[{}]\n]", body.trim());
            return;
        }
    }
    out.push(')');
}

/// The block kinds this backend renders as blocks. Used to tell an inline run
/// from a block run in a container whose content model is either.
fn is_block_kind(kind: &twig::Kind) -> bool {
    matches!(
        kind,
        twig::Kind::Para
            | twig::Kind::Heading
            | twig::Kind::BlockQuote
            | twig::Kind::CodeBlock
            | twig::Kind::RawBlock
            | twig::Kind::BulletList
            | twig::Kind::OrderedList
            | twig::Kind::TaskList
            | twig::Kind::DefinitionList
            | twig::Kind::Table
            | twig::Kind::LineBlock
            | twig::Kind::ThematicBreak
            | twig::Kind::Container
            | twig::Kind::Section
    )
}

/// Render children that may be inline, blocks, or both.
fn render_mixed(out: &mut String, doc: &Doc, cx: &Cx, kids: &[Idx]) {
    if kids.iter().any(|&k| is_block_kind(&doc.node(k).kind)) {
        for &k in kids {
            emit_block(out, doc, cx, k);
        }
    } else {
        emit_inlines(out, doc, cx, kids);
    }
}

/// A checkbox drawn rather than spelled.
///
/// `☐`/`☑` are the obvious characters and the wrong choice: they are absent
/// from most serif text fonts, so an unchecked box renders as tofu under the
/// default font while the checked one silently falls back to some other face.
/// Drawing the box makes it font-independent and visually consistent.
fn checkbox(checked: bool) -> &'static str {
    if checked {
        "#box(width: 0.62em, height: 0.62em, radius: 1pt, stroke: 0.5pt, \
         fill: black, inset: 1pt, baseline: 0.05em)"
    } else {
        "#box(width: 0.62em, height: 0.62em, radius: 1pt, stroke: 0.5pt, \
         baseline: 0.05em)"
    }
}

fn emit_inlines(out: &mut String, doc: &Doc, cx: &Cx, kids: &[Idx]) {
    for &k in kids {
        emit_inline(out, doc, cx, k);
    }
}

fn emit_inline(out: &mut String, doc: &Doc, cx: &Cx, idx: Idx) {
    if cx.is_hidden(doc, idx) {
        return;
    }
    let n = doc.node(idx);
    let empty = Style::default();
    let style = cx.style(idx).unwrap_or(&empty);

    let mut inner = String::new();
    match n.kind.as_str() {
        "str" | "text_leaf" => {
            inner.push_str(&cx.text(doc, idx, n.str_field("text").unwrap_or_default()))
        }
        "soft_break" => inner.push('\n'),
        "hard_break" => inner.push_str(" \\\n"),
        "non_breaking_space" => inner.push('~'),

        "emph" => {
            inner.push_str("#emph[");
            emit_inlines(&mut inner, doc, cx, &n.children);
            inner.push(']');
        }
        "strong" => {
            inner.push_str("#strong[");
            emit_inlines(&mut inner, doc, cx, &n.children);
            inner.push(']');
        }
        "verbatim" | "code" => {
            let text = n.str_field("text").unwrap_or_default();
            inner.push_str(&cx.code(doc, idx, text, None, false));
        }
        "link" => {
            let dest = n.str_field("destination").unwrap_or_default();
            let _ = write!(inner, "#link({})[", quote(dest));
            if n.children.is_empty() {
                inner.push_str(&cx.text(doc, idx, dest));
            } else {
                emit_inlines(&mut inner, doc, cx, &n.children);
            }
            inner.push(']');
        }
        "image" => {
            let dest = n.str_field("destination").unwrap_or_default();
            let mut alt = String::new();
            emit_inlines(&mut alt, doc, &cx.plain(), &n.children);
            let alt = strip_markup(&alt);
            match image_source(dest) {
                Some(src) if alt.is_empty() => {
                    let _ = write!(inner, "#image({src})");
                }
                Some(src) => {
                    let _ = write!(inner, "#image({src}, alt: {})", quote(&alt));
                }
                // Nothing Typst can read: the alt text stands in, rather
                // than the whole document failing to compile.
                None if alt.is_empty() => inner.push_str(&cx.text(doc, idx, dest)),
                None => inner.push_str(&cx.text(doc, idx, &alt)),
            }
        }
        "footnote_reference" => {
            let label = n.str_field("text").unwrap_or_default();
            match find_footnote(doc, label) {
                Some(def) => {
                    inner.push_str("#footnote[");
                    let mut body = String::new();
                    for &c in &doc.node(def).children {
                        emit_block(&mut body, doc, cx, c);
                    }
                    inner.push_str(body.trim());
                    inner.push(']');
                }
                None => {
                    let _ = write!(inner, "#super[{}]", escape(label));
                }
            }
        }
        "smart_punctuation" => {
            let punct = smart_punct(n.str_field("kind").unwrap_or_default());
            inner.push_str(&cx.text(doc, idx, punct));
        }
        "raw_inline" => {
            if matches!(n.str_field("format"), Some("typst") | Some("pdf")) {
                inner.push_str(n.str_field("text").unwrap_or_default());
            }
        }
        // Inline containers (a djot span, an HTML `<span>`, a `:name` text
        // directive) carry no rendering of their own — they exist so a rule
        // can attach one.
        _ => emit_inlines(&mut inner, doc, cx, &n.children),
    }

    out.push_str(&apply_inline_style(&inner, style));
}

/// Wrap inline content in the text-level properties a style sets.
fn apply_inline_style(content: &str, style: &Style) -> String {
    let mut out = content.to_string();

    if let Some(raw) = &style.raw
        && let Some(wrap) = &raw.wrap
    {
        out = format!("#{wrap}[{out}]");
    }

    let args = text_args(style);
    if !args.is_empty() {
        out = format!("#text({})[{out}]", args.join(", "));
    }
    if style.italic == Some(true) {
        out = format!("#emph[{out}]");
    }
    if style.underline == Some(true) {
        out = format!("#underline[{out}]");
    }
    if style.strike == Some(true) {
        out = format!("#strike[{out}]");
    }
    if style.smallcaps == Some(true) {
        out = format!("#smallcaps[{out}]");
    }

    if let Some(raw) = &style.raw {
        let before = raw.before.as_deref().unwrap_or("");
        let after = raw.after.as_deref().unwrap_or("");
        if !before.is_empty() || !after.is_empty() {
            out = format!("{before}{out}{after}");
        }
    }
    out
}

fn text_args(style: &Style) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(f) = &style.font {
        args.push(format!("font: {}", quote(f)));
    }
    if let Some(s) = &style.size {
        args.push(format!("size: {s}"));
    }
    if let Some(w) = style.weight {
        args.push(format!("weight: {}", quote(weight_str(w))));
    }
    if let Some(c) = &style.color {
        args.push(format!("fill: {}", color(c)));
    }
    if let Some(t) = &style.tracking {
        args.push(format!("tracking: {t}"));
    }
    args
}

/// Wrap block content in the block-level properties a style sets.
fn wrap_block(content: &str, style: &Style, doc: &Doc, idx: Idx) -> String {
    let content = content.trim_end();
    if content.is_empty() {
        return String::new();
    }
    let mut out = content.to_string();

    if let Some(raw) = &style.raw
        && let Some(wrap) = &raw.wrap
    {
        let wrap = interpolate_node(wrap, doc, idx);
        out = format!("#{wrap}[\n{}\n]", indent(&out));
    }

    let args = text_args(style);
    if !args.is_empty() {
        out = format!("#text({})[\n{}\n]", args.join(", "), indent(&out));
    }
    if style.italic == Some(true) {
        out = format!("#emph[\n{}\n]", indent(&out));
    }

    let mut block_args: Vec<String> = Vec::new();
    if let Some(b) = &style.box_ {
        if let Some(f) = &b.fill {
            block_args.push(format!("fill: {}", color(f)));
        }
        if let Some(s) = &b.stroke {
            let w = b.stroke_width.as_deref().unwrap_or("0.5pt");
            block_args.push(format!("stroke: {w} + {}", color(s)));
        } else if let Some(w) = &b.stroke_width {
            block_args.push(format!("stroke: {w}"));
        }
        if let Some(p) = &b.padding {
            block_args.push(format!("inset: {p}"));
        }
        if let Some(r) = &b.radius {
            block_args.push(format!("radius: {r}"));
        }
        if let Some(w) = &b.width {
            block_args.push(format!("width: {w}"));
        }
    }
    if let Some(s) = &style.space_before {
        block_args.push(format!("above: {s}"));
    }
    if let Some(s) = &style.space_after {
        block_args.push(format!("below: {s}"));
    }

    let needs_pad = style.margin_left.is_some() || style.margin_right.is_some();
    if !block_args.is_empty() {
        out = format!("#block({})[\n{}\n]", block_args.join(", "), indent(&out));
    }
    if needs_pad {
        let mut pad = Vec::new();
        if let Some(l) = &style.margin_left {
            pad.push(format!("left: {l}"));
        }
        if let Some(r) = &style.margin_right {
            pad.push(format!("right: {r}"));
        }
        out = format!("#[\n#set pad({})\n{}\n]", pad.join(", "), indent(&out));
    }

    if let Some(l) = &style.leading {
        out = format!("#[\n#set par(leading: {l})\n{}\n]", indent(&out));
    }
    if let Some(i) = &style.indent {
        out = format!("#[\n#set par(first-line-indent: {i})\n{}\n]", indent(&out));
    }
    if let Some(a) = style.align
        && a != Align::Justify
    {
        let a = match a {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => unreachable!(),
        };
        out = format!("#[\n#set align({a})\n{}\n]", indent(&out));
    }
    out
}

// ── Text helpers ───────────────────────────────────────────────────────────

/// Escape text for Typst markup mode.
///
/// Typst reads `\` + ASCII punctuation as that punctuation literally, so
/// escaping the whole set is safe and much easier to be confident in than
/// tracking which characters are special in which position. `-` and `+` are
/// only list markers at the start of a line, but a paragraph reflowed by a
/// later edit can put any run at the start of a line, so they are escaped
/// unconditionally too. A `.` is escaped where another follows it, since
/// `...` is Typst's shorthand for an ellipsis.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        // Not every `.`: only one before another, since `...` is Typst's
        // shorthand for an ellipsis and the author wrote three dots.
        if c == '.' && chars.peek() == Some(&'.') {
            out.push('\\');
        }
        if matches!(
            c,
            '\\' | '#'
                | '$'
                | '*'
                | '_'
                | '`'
                | '<'
                | '>'
                | '@'
                | '~'
                | '['
                | ']'
                | '"'
                | '\''
                | '='
                | '-'
                | '+'
                | '/'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A Typst string literal.
/// What `#image` reads a picture from: a path, resolved against the
/// document's folder by the `World`; or a `data:` URL's bytes, written into
/// the source, since Typst reads files and not URLs. `None` for what Typst
/// cannot read at all: a remote URL (Typst does not fetch), or a `data:`
/// URL that does not decode.
fn image_source(dest: &str) -> Option<String> {
    use base64::Engine as _;
    let lower = dest.get(..8).unwrap_or(dest).to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return None;
    }
    let Some(url) = dest.strip_prefix("data:") else {
        return Some(quote(dest));
    };
    let (meta, data) = url.split_once(',')?;
    let bytes = if meta.ends_with(";base64") {
        let clean: String = data.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(clean)
            .ok()?
    } else {
        percent_decode(data)?
    };
    let mut out = String::from("bytes((");
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{b}");
    }
    // A one-element array needs its comma, or the parentheses are grouping.
    if bytes.len() == 1 {
        out.push(',');
    }
    out.push_str("))");
    Some(out)
}

/// A `data:` URL's percent-encoded body as bytes.
fn percent_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Frontmatter data as a Typst value: strings stay strings, never markup, so
/// a document's metadata cannot run as code in its template.
fn typst_value(v: &serde_json::Value) -> String {
    use serde_json::Value as Json;
    match v {
        Json::Null => "none".into(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => n.as_f64().map_or("none".into(), |f| format!("{f:?}")),
        },
        Json::String(s) => quote(s),
        Json::Array(items) => {
            let items: Vec<String> = items.iter().map(typst_value).collect();
            format!("({},)", items.join(", "))
        }
        Json::Object(map) if map.is_empty() => "(:)".into(),
        Json::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", quote(k), typst_value(v)))
                .collect();
            format!("({})", fields.join(", "))
        }
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One piece of a raw-Typst string as `scan` sees it: literal text to copy
/// verbatim, or a well-formed `{{...}}` placeholder's trimmed inner text.
enum Piece<'a> {
    Literal(&'a str),
    Placeholder(&'a str),
}

/// Split `text` into literal spans and placeholder tokens, in order — the one
/// implementation of the `{{...}}` grammar, shared by [`interpolate`] (which
/// substitutes placeholders) and [`crate::schema`] (which only needs to see
/// the tokens, to validate them without a matched node to resolve against).
///
/// `{{` with no matching `}}` is not a placeholder: the rest of the string
/// from that point on is one final literal span, same as any raw Typst that
/// simply never mentioned the grammar at all.
fn scan(text: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        if start > 0 {
            pieces.push(Piece::Literal(&rest[..start]));
        }
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                pieces.push(Piece::Placeholder(after[..end].trim()));
                rest = &after[end + 2..];
            }
            None => {
                pieces.push(Piece::Literal(&rest[start..]));
                rest = "";
            }
        }
    }
    if !rest.is_empty() {
        pieces.push(Piece::Literal(rest));
    }
    pieces
}

/// Every well-formed `{{...}}` placeholder token in `text`, in order — what
/// `crate::schema`'s syntax check validates.
pub(crate) fn placeholder_tokens(text: &str) -> Vec<&str> {
    scan(text)
        .into_iter()
        .filter_map(|p| match p {
            Piece::Placeholder(key) => Some(key),
            Piece::Literal(_) => None,
        })
        .collect()
}

/// Replace every `{{source.name}}` placeholder in a raw-Typst string with the
/// value `resolve` returns for it, quoted as a Typst string literal (absent
/// -> `""`).
///
/// Always quoting is the whole safety property: this is the one place
/// document-derived text (an attribute value, a bound frontmatter field)
/// reaches raw Typst source rather than markup `escape`d text, so a crafted
/// value cannot close the literal and inject Typst of its own. A rule that
/// wants a non-string value asks Typst to convert it — `#lorem(int({{attr.count}}))`
/// — rather than this function ever splicing something unquoted.
///
/// Not a parser: `{{` with no matching `}}` is left as literal text, and an
/// empty/unmatched key resolves to `""` rather than erroring, matching how an
/// unfired `from-metadata` binding is reported (`--explain`) rather than
/// rejected — a placeholder that never matches any node is a config that
/// still renders.
fn interpolate(text: &str, resolve: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in scan(text) {
        match piece {
            Piece::Literal(s) => out.push_str(s),
            Piece::Placeholder(key) => out.push_str(&quote(&resolve(key).unwrap_or_default())),
        }
    }
    out
}

/// Interpolate against one matched node: `{{attr.NAME}}` (a directive/HTML
/// attribute), `{{field.NAME}}` (a typed payload field — `level`,
/// `destination`, `lang`, …), `{{text}}` (concatenated descendant text).
fn interpolate_node(text: &str, doc: &Doc, idx: Idx) -> String {
    interpolate(text, |key| {
        if key == "text" {
            return Some(doc.text_of(idx));
        }
        let n = doc.node(idx);
        let (source, name) = key.split_once('.')?;
        match source {
            "attr" => n.attr(name).flatten().map(str::to_string),
            "field" => n
                .str_field(name)
                .map(str::to_string)
                .or_else(|| n.int_field(name).map(|i| i.to_string()))
                .or_else(|| n.bool_field(name).map(|b| b.to_string())),
            _ => None,
        }
    })
}

/// Interpolate against the resolved document config: `{{doc.NAME}}`, the same
/// names `[document.from-metadata]` binds into — see `Document::field_as_str`.
fn interpolate_doc(text: &str, d: &Document) -> String {
    interpolate(text, |key| d.field_as_str(key.strip_prefix("doc.")?))
}

/// A colour literal. Hex becomes `rgb("#…")`; a bare name passes through as a
/// Typst colour identifier.
fn color(c: &str) -> String {
    let c = c.trim();
    if c.starts_with('#') {
        format!("rgb({})", quote(c))
    } else {
        c.to_string()
    }
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("  {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Indent every line but the first, so a multi-line body sits under its list
/// marker instead of terminating the item.
fn indent_continuation(s: &str) -> String {
    let s = s.trim_end();
    let mut lines = s.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    let rest: Vec<String> = lines
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("  {l}")
            }
        })
        .collect();
    if rest.is_empty() {
        first.to_string()
    } else {
        format!("{first}\n{}", rest.join("\n"))
    }
}

/// Crude markup strip for an `alt` attribute, which must be a plain string.
fn strip_markup(s: &str) -> String {
    s.replace('\\', "")
        .replace(['[', ']'], "")
        .trim()
        .to_string()
}

/// Map twig's `ordered_list.numbering` to a Typst numbering pattern.
fn numbering_pattern(numbering: Option<&str>) -> Option<&'static str> {
    match numbering? {
        "decimal" => None, // Typst's default; emitting it is noise.
        "lower_alpha" => Some("a."),
        "upper_alpha" => Some("A."),
        "lower_roman" => Some("i."),
        "upper_roman" => Some("I."),
        _ => None,
    }
}

fn smart_punct(kind: &str) -> &'static str {
    match kind {
        "left_single_quote" => "‘",
        "right_single_quote" => "’",
        "left_double_quote" => "“",
        "right_double_quote" => "”",
        "ellipsis" => "…",
        "en_dash" => "–",
        "em_dash" => "—",
        _ => "",
    }
}

/// Find the `footnote` definition whose label matches `label`.
fn find_footnote(doc: &Doc, label: &str) -> Option<Idx> {
    doc.nodes.iter().position(|n| {
        n.kind == twig::Kind::Footnote
            && n.str_field("label").or(n.str_field("text")) == Some(label)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_neutralises_typst_markup() {
        assert_eq!(escape("a*b_c#d"), "a\\*b\\_c\\#d");
        assert_eq!(escape("wait... ok."), "wait\\.\\.. ok.");
        assert_eq!(escape("100% of $5"), "100% of \\$5");
    }

    #[test]
    fn raw_content_survives_quoting_and_indentation() {
        // The reason code blocks are emitted as `#raw(...)` calls: the
        // argument is a string, so backticks and leading whitespace in the
        // content cannot terminate or reflow it.
        assert_eq!(quote("a ``b`` c"), "\"a ``b`` c\"");
        assert_eq!(quote("  indented\n"), "\"  indented\\n\"");
    }

    #[test]
    fn hex_colors_become_rgb_and_names_pass_through() {
        assert_eq!(color("#eef"), "rgb(\"#eef\")");
        assert_eq!(color("navy"), "navy");
    }

    #[test]
    fn continuation_lines_stay_inside_their_list_item() {
        assert_eq!(indent_continuation("first\nsecond\n"), "first\n  second");
    }

    #[test]
    fn interpolate_substitutes_a_quoted_value() {
        let out = interpolate("#lorem(int({{attr.count}}))", |k| {
            assert_eq!(k, "attr.count");
            Some("150".into())
        });
        assert_eq!(out, "#lorem(int(\"150\"))");
    }

    #[test]
    fn interpolate_leaves_text_without_placeholders_untouched() {
        assert_eq!(interpolate("#major-break", |_| None), "#major-break");
    }

    #[test]
    fn interpolate_resolves_an_unmatched_key_to_an_empty_string() {
        assert_eq!(interpolate("[{{attr.missing}}]", |_| None), "[\"\"]");
    }

    #[test]
    fn interpolate_leaves_an_unterminated_placeholder_as_literal_text() {
        assert_eq!(interpolate("a {{b", |_| Some("x".into())), "a {{b");
    }

    /// The safety property the whole design leans on: a resolved value that
    /// itself contains a `"` cannot close the Typst string literal early.
    #[test]
    fn interpolate_always_quotes_so_a_value_cannot_break_out_of_the_literal() {
        let out = interpolate("#f({{x}})", |_| Some("a\" + read(\"/etc/passwd".into()));
        assert_eq!(out, "#f(\"a\\\" + read(\\\"/etc/passwd\")");
    }
}
