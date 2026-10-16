---
title: Twig Export
author: adammharris
created: 2026-08-13T16:28:00-07:00
updated: 2026-08-13T17:40:00-07:00
contents: ['[DESIGN](/DESIGN.md)']
---

# Twig Export

Render [Twig](https://github.com/diaryx-org/twig) document ASTs to **Typst**
and **PDF**, with a configurable map from in-text directives and document
metadata to styling.

```sh
twig-export report.md -c style.toml --directives -o report.pdf
twig-export report.md -c style.toml --directives -o report.typ   # inspect the Typst
twig-export report.md -c style.toml --directives --explain       # why didn't my rule fire?
```

## Why a separate project

Twig's charter rules out first-class presentation formats — but its
architecture already anticipates one. `crates/twig/src/format.rs` splits
`Format` (what Twig can *parse*) from `Target` (what Twig can *write*)
precisely so an **export-only** target is expressible, and Twig's
`docs/DESIGN.md` names PDF as the motivating case. This crate is that target, built outside
Twig's tree where the presentation concerns belong. It consumes Twig's
published AST and adds nothing to Twig's parse surface.

## Why Typst rather than handrolling

Writing a PDF container is the easy half. The other half is a typesetting
engine — font subsetting and embedding, shaping, line breaking, hyphenation,
pagination with widow and orphan control, footnote carry-over — and none of it
is document-AST work.

The deciding argument is the config, though. Typst's `set`/`show` rules are
already a selector→styling language, so the style map *compiles to* something
instead of being interpreted by a layout engine that would also have to be
written. Handrolling means inventing the vocabulary **and** implementing every
property in it.

`.typ` is the real target and PDF is a compile step on top. That keeps the
whole interesting surface text-to-text, so it tests with golden files rather
than by diffing binary PDFs — and a `.typ` file is a useful artifact for
anyone who wants to take the result further by hand.

## The config

Written in any language [`fig`](https://github.com/diaryx-org/fig) speaks —
`.toml`, `.yaml`, `.json`, `.figl` — because the same parser reads the
document's frontmatter, whose language is whatever the author's fence tag said.

A rule's `select` is a **Twig selector**, resolved by Twig's own engine. So
anything you can find with `twig query 'container[name=note][class~=warn]'` is
something you can style, with identical semantics and no second grammar to
drift.

```toml
[document]
paper = "a4"
margin = "2.5cm"
page-numbering = "1"

# Frontmatter keys → document settings. A key nothing binds is inert.
[document.from-metadata]
title = "title"
paper = "pdf-paper"

[[rule]]
select = "heading[level=1]"
size = "20pt"
weight = "bold"

[[rule]]
select = "container[name=note]"

[rule.box]
fill = "#eef2ff"
stroke = "#8899cc"
padding = "0.8em"
radius = "3pt"

# Only the fill is restated; stroke, padding and radius are inherited.
[[rule]]
select = "container[name=note][class~=warn]"

[rule.box]
fill = "#fff0f0"

# Audience filtering: drop a directive and its whole subtree.
[[rule]]
select = "container[name=internal]"
hidden = true
```

Rules apply in **source order**, each overlaying the last property by
property. There is deliberately no CSS-style specificity — put the general
rule first and the specific one after.

### Config diagnostics

The config is validated by [`fig-schema`](https://crates.io/crates/fig-schema)
against the value tree *before* deserialization, so every problem is reported
with its path and a near-miss suggestion, and all of them at once:

```
twig-export: in config style.toml: invalid config:
  rule[0].weigth: “weigth” is not a known value — did you mean “weight”?
  rule[1].weight: “semibol” is not a known value — did you mean “semibold”?
  rule[1].box.padding: “3” has no unit — expected a number followed by pt, mm, cm, in, em, %, fr
  rule[1].box.fill: “#xyz” is not a #rgb / #rrggbb / #rrggbbaa colour
```

Vocabularies are closed where the set really is fixed (`weight`, `align`,
`break-before`) and open where it is not (`paper`, `font`). A closed one
rejects; an open one warns and passes the value through to Typst:

```
twig-export: warning: document.paper: “a9” is not a known value
```

The style vocabulary is **backend-neutral** (`font`, `size`, `weight`,
`leading`, `space-before`, `align`, `break-before`, `box`, `hidden`, `unwrap`,
…), so a future CSS-paged-media backend could honour the same file. Anything
it cannot express goes through a `[rule.raw]` block that names Typst
explicitly, so reading a config makes obvious which parts are portable.

## The document's half

The config decides what a document is *allowed* to say about itself. A
document carrying `pdf-paper: a3` changes nothing unless the config bound it,
and a directive no rule mentions does nothing. The config is the authority and
the document is the input — which is what makes it safe to render someone
else's document.

## Building

```sh
cargo build                          # with the in-process Typst compiler
cargo build --no-default-features    # Typst emission only; no PDF, builds fast
cargo build --features html          # + experimental HTML export
cargo test
```

The `pdf` feature links the Typst compiler in-process, so a single binary
produces PDF with no external toolchain. Without it the crate still emits
`.typ`, which is where all the logic worth testing lives.

The `html` feature does the same through Typst's own HTML backend
(`-o out.html`), but is opt-in rather than default: Typst calls that backend
"under active development and incomplete," and it silently drops the
*content* (not just the styling) of a couple of PDF-oriented constructs — see
`src/html.rs` for what `typst_emit.rs` already works around and what to check
before trusting a new one.

## Status

Covered: headings, paragraphs, emphasis/strong/code, links, images, block
quotes, code blocks with language, bullet/ordered/task/definition lists,
tables with alignment and spans, line blocks, footnotes, thematic breaks,
containers and directives, metadata islands, smart punctuation.

Not yet: math (`--math` parses it; the emitter does not map it), citations,
and per-page furniture beyond a page-number pattern.

## License

Licensed under either of Apache License 2.0 or MIT license at your option.
