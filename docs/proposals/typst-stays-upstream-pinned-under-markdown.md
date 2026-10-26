---
title: Typst stays upstream, pinned, under Markdown
description: Typst as a thin layer means the author never writes Typst and the only Typst code is a template for the procedural part of a page — a shape of use, not a cut of the code — so twig-export takes Typst from crates.io at an exact version, keeps every reach into its internals in one module with a canary test, asks upstream for the two things it lacks, and forks only on a named trigger
status: draft
created: 2026-10-26
updated: 2026-10-26
part_of: '[Proposals](/docs/proposals/proposals.md)'
---

# Typst stays upstream, pinned, under Markdown

## Status

`draft`, argued 2026-10-26 from twig-export `main` at `bf145b7` (twig 6,
fig 7, Typst 0.15.1). The binary sizes below were measured on this branch, on
an Apple Silicon Mac. Nothing else is built.

This proposal follows two others and does not repeat them:

- leaf's [What you see is what twig-export sets](https://github.com/diaryx-org/leaf/blob/main/docs/proposals/what-you-see-is-what-twig-export-sets.md)
  (draft) argues that leaf's page view draws twig-export's Typst pages, and
  that the words reach Typst as *content built from the twig tree*, not as
  generated `.typ`. Its spike (`spikes/twig-export-live`) measured the speed
  and the spans. It lists the twig-export work under "What twig-export
  needs".
- Diaryx's [A document leaves through one typesetter](https://github.com/diaryx-org/diaryx/blob/main/docs/proposals/a-document-leaves-through-one-typesetter.md)
  (draft) makes twig-export the route for every format that is set on pages.

This one answers the question those two leave open: how twig-export holds
Typst.

## The ask

Adam, 2026-10-26: "Perhaps we can fork Typst (or at least pin) so we can use
it specifically as a thin procedural typesetting instructions layer, with
Markdown/directives as the main content interface."

So there are two questions:

1. What does "thin" mean in practice: which parts of Typst stay, and which
   does the author never see?
2. How does twig-export hold Typst: a fork, an exact pin, or a pin with
   patches? What does each cost, and how does a Typst release reach us?

## The short answer

**"Thin" is a shape of use, not a cut of the code.** The author writes
Markdown or djot with directives. twig-export builds the body as Typst
content in Rust, so no Typst markup is made for the words. The only Typst
code is a *template*: the contents page, the running heads, the page numbers,
the heading numbers. Typst's engine stays whole, because the engine is what
makes the page exact. Typst's language stays too, because a template needs
`context`, `query` and `outline`. Only the author stops seeing it.

**Pin, do not fork, and fork only on a named trigger.**

| | What it is | Cost per Typst release | Verdict |
| --- | --- | --- | --- |
| **Fork** | `diaryx-org/typst`, a branch of patches over an upstream tag | Rebase the patches; build and test the fork; every consumer takes Typst from git, so twig-export cannot go to crates.io | Not now. Keep the shape ready (**When to fork**) |
| **Exact pin** | `typst = "=0.15.1"` and its siblings, from crates.io | Bump the version; fix what moved in one module; run the canary tests | **Yes** |
| **Pin + vendored patches** | One or two Typst crates copied into the repo and swapped in with `[patch.crates-io]` | As the fork, and each consumer must repeat the `[patch]` | Only as the first step of a fork |

The content route needs nothing that 0.15.1 does not already make public,
except one span constructor, which twig-export can pack by hand and test
(**What twig-export reaches into**). So a fork buys nothing yet. It would buy
a smaller binary, about 10 MB of 47 MB (**What a cut would save**). That is
worth a fork only if a phone needs it.

## What "thin" means

Three layers, and who sees each:

| Layer | What it holds | Who writes it | Typst parts it uses |
| --- | --- | --- | --- |
| **Content** | The words, marks, headings, lists, tables, footnotes, pictures, directives | The author, in Markdown or djot | None. twig parses it |
| **Template** | The procedural part of a page: the paper, the contents page, running heads, page numbers, heading and footnote numbering, the title page | Diaryx ships one; a book may name its own | The language: `set`, `show`, `context`, `query`, `counter`, `outline` |
| **Engine** | Line breaking, hyphenation, pagination, footnote placement, introspection, fonts, PDF | Nobody | `typst-library`, `typst-realize`, `typst-layout`, `typst-pdf` |

The style config stays between content and template. It maps twig selectors
to a neutral style vocabulary, and twig-export applies the result as Typst
styles on the content it builds. The `[rule.raw]` escape hatch stays visible
and names the backend at the point of use (`DESIGN.md`, "The escape hatch is
deliberately visible"). Under the content route its `before`/`after`/`wrap`
cannot splice markup, so its uses move into the template, as show rules.

### What the author sees

- **Markdown or djot, with directives.** That is the whole of a document.
- **Errors that point at the Markdown.** A glyph built from a twig node
  carries a span into the Markdown source (the leaf spike found 99.9% of
  glyphs on their exact byte). A diagnostic about that content carries the
  same span, so a warning about a word names a line of the document, not a
  line of generated `.typ`. A diagnostic about the template names the
  template. Today `pdf.rs` must quote the
  generated source and tell the user to read `-o out.typ`.
- **A template only if they ask for one.** A document with no `template:`
  gets Diaryx's. A template is a file in the library, in Typst's sandbox,
  which reads only the files the `World` gives it.
- **`.typ` only as an export.** `typst_emit.rs` stays, for a person who takes
  a book to a typesetter by hand. It is no longer the step every PDF goes
  through.

### What stays in the binary and is never used

Typst's crates have no feature flags. `typst-library`, `typst-layout`,
`typst-pdf` and `typst-eval` declare none, so every part of the standard
library is linked whether a document uses it or not. These parts have no use
in the content route today:

| Part | Crates | Used by twig-export? |
| --- | --- | --- |
| Bibliographies and citations | `hayagriva`, `citationberg`, `biblatex` | No. Possible later, if twig gains citations |
| WebAssembly plugins | `wasmi`, `wasmparser` | No. A template could load one, from a file the `World` serves |
| A PDF as a picture | `hayro-*` | No, unless a document embeds a PDF |
| HTML export | `typst-html` | Only behind the `html` feature, but the `typst` crate links it always |
| Typst Universe downloads | `typst-kit/system-downloader` | Off in Diaryx builds; already behind `network` |

Code highlighting (`syntect`, `two-face`) and math stay: twig-export emits
code blocks today, and math is the first gap the leaf proposal lists.

## What a cut would save

Measured on this branch: `cargo bloat --release --no-default-features
--features pdf --bin twig-export --crates`, then `strip`.

| | Size |
| --- | --- |
| Binary, as built | 56.6 MiB |
| Binary, stripped | 47.1 MB (the leaf spike: 46 MB) |
| Machine code (`__text`) | 20.0 MiB |
| Constant data (`__const`) | 18.7 MB |
| typst-kit's embedded fonts, on disk | 9.6 MiB |
| syntect's syntax and theme dumps (`two-face`), on disk | 3.6 MiB |

Machine code of the unused parts, from `cargo bloat`:

| Part | Machine code |
| --- | --- |
| `citationberg` + `hayagriva` + `biblatex` | 1.0 + 1.0 + 0.15 MiB |
| `wasmi` + `wasmparser` | 0.70 + 0.37 MiB |
| `hayro-*` (syntax, interpret, jbig2, svg, jpeg2000, ...) | about 1.5 MiB |
| `typst-html` | 0.17 MiB |
| **Total** | **about 4.9 MiB** |

Add the bibliography's bundled CSL styles and locales, which are data, and a
cut would remove about 5 to 7 MiB of the 47 MB. That is an estimate:
`cargo bloat` says its numbers are "guesswork", and the data was not
attributed per crate.

The fonts are a larger lever, and they need no fork. They are typst-kit's
`embedded-fonts` feature. The Diaryx proposal already wants one bundled face
for each generic family, so Diaryx would swap typst-kit's set for its own.

So a fork saves about an eighth of the binary. That matters on a phone, if
at all. On the Mac and the CLI it does not.

## What twig-export reaches into

The content route uses Typst as a Rust library, not through its text format.
The leaf spike's `content.rs` is the complete list of what it touches. All of
it is public in 0.15.1, except one item:

| What | From | Public in 0.15.1? |
| --- | --- | --- |
| `World`, `Library::builder`, `LazyHash`, `Source`, `FileId` | `typst`, `typst-syntax` | Yes |
| Elements: `HeadingElem`, `TextElem`, `StrongElem`, `EmphElem`, `ListElem`, `FootnoteElem`, ... and `Content::sequence`, `.spanned()` | `typst-library` | Yes |
| The template's evaluation: `typst_eval::eval`, the module's scope, `Func::call` | `typst-eval`, `typst-library` | Yes |
| The layout loop: `Engine` (all fields `pub`), `Introspector`, `comemo::Constraint`, `MAX_ITERS`, `PagedDocument::create` | `typst-library`, `typst-layout` | Yes. The loop itself is `compile_impl`, which is private, so twig-export copies its 60 lines |
| A span that is a block and an offset: `Span::from_range` | `typst-syntax` | **No**, `pub(crate)`. Packed by hand through the public `Span::from_raw` |
| A glyph's span, for click to source | `typst-library` (`FrameItem::Text`, `Glyph::span`) | Yes |
| PDF: `typst_pdf::pdf` | `typst-pdf` | Yes |

"Public" is not "stable". Typst is 0.x, and each minor release may move any
of these. The spike's world code already shows it: `RootedPath` and
`VirtualRoot` are new in 0.15. Two of these items are copies of Typst's own
code, not calls to it: the span packing and the layout loop. A copy does not
break at compile time when Typst changes what it copied. It goes quietly
wrong. Those two need tests, not only a build.

## The proposal

### 1. Pin Typst exactly, from crates.io

Write every Typst crate with `=`:

```toml
typst = { version = "=0.15.1", optional = true }
typst-pdf = { version = "=0.15.1", optional = true }
typst-layout = { version = "=0.15.1", optional = true }
typst-eval = { version = "=0.15.1", optional = true }  # the content route
typst-kit = { version = "=0.15.1", optional = true, features = [...] }
typst-html = { version = "=0.15.1", optional = true }
```

Today the manifest says `"0.15"`, and `Cargo.lock` holds 0.15.1. A lock file
pins twig-export's own builds, but not its consumers: leaf and Diaryx
resolve `"0.15"` themselves, and a 0.15.2 could arrive there first. With `=`,
a consumer that also links Typst (leaf's page view will, for `typst-render`
or the frames) must resolve the same version, or Cargo refuses. That is the
outcome we want. Two Typst versions in one graph is two engines, which is the
thing the leaf proposal exists to avoid.

### 2. One module holds every reach into Typst's internals

A new `src/typst_seam.rs` holds everything in the table above that is not a
plain call: the span packing, the copied layout loop, and the `World`
(`typst_world.rs` moves into it or beside it). The content builder calls only
the seam and the element constructors. A Typst bump then has one place to
read and one place to fix.

### 3. Canary tests for what is copied

Each copy gets a test that fails if Typst changes under it:

- **Span packing.** Pack a block and an offset; `Span::get()` must return
  `SpanKind::Range` with the same file and the same two numbers. If Typst
  changes the bit layout, this fails before any caret lands on the wrong
  word.
- **Layout loop.** Set one document through the seam's loop and through
  `typst::compile`, the second as a `.typ` that imports the same template and
  calls `book` with the same body. They must give the same pages: the same
  count, and the same glyphs at the same positions. If `compile_impl` gains
  a step (a warning pass, a new target), the pages or the warnings differ.
- **Content against `.typ`.** The leaf proposal's test: the corpus set
  through the content builder and through `typst_emit.rs` gives the same
  pages. It keeps the two walks of the tree in agreement, and it catches an
  element whose defaults moved.

The existing golden `.typ` tests and `tests/pdf.rs` stay.

### 4. Ask upstream for the two things we copy

Two requests to `typst/typst`, as the leaf proposal names:

1. **A public range-span constructor**: make `Span::from_range` `pub`. It is
   a one-word change. `DiagSpan::from_range` is already public, for
   diagnostics only, so the idea is not new to Typst.
2. **A compile entry point that takes content beside the main file**: for
   example, `typst::compile_with(world, |engine, module| -> Content)`, or a
   `compile_impl` that is public under a name that says it is low-level.

With both, the seam shrinks to the `World` and twig-export copies nothing.
Each is an issue first, a PR only if the maintainers want one. Sending either
publishes, so each needs Adam's approval.

### 5. How a Typst release is absorbed

One PR in twig-export, then one bump in each consumer:

1. Read the release's changelog for the elements, `Span`, `Engine`,
   `Introspector`, `World` and `compile`.
2. Change every `=0.15.1` to the new version. Build. Fix the compile errors,
   which should all be in `typst_seam.rs` and the element constructors.
3. Run `cargo xtask ci`, or what CI runs. The canary tests say whether a copy
   went wrong. The golden `.typ` diff shows what the emitter must change,
   if anything. Review a PDF of `examples/report.md` by eye, because a layout
   change is not an error.
4. Merge. Then, in leaf and Diaryx, `cargo update -p twig-export` and the
   same `=` change wherever they name Typst themselves, in the same session,
   so no app is on two Typst versions.

Typst's minor releases have come a few months apart. A bump is a planned
afternoon, not a background chore, and nothing forces one. A security fix is
the exception (**When to fork**).

### 6. When to fork, and its shape

Fork when one of these is true, and not before:

- **Upstream declines a request, and the copy breaks twice.** A copy that
  survives releases is cheaper than a fork. A copy that breaks every release
  is not.
- **A fix is needed before Typst releases it.** For example, a layout bug in
  a shipped book, or a security fix.
- **A phone needs the binary smaller.** The measurement in **What a cut
  would save** decides it. Cutting the bibliography, plugins and PDF images
  means removing their elements from the library's scope. The direct imports
  are few (two files for `hayagriva`, one for `wasmi`, two for `hayro`), but
  the elements they back are named in layout, realize, PDF and HTML export.
  That patch is unmeasured, and it is larger than the other two triggers.

The shape, when it comes:

- **A fork repository**, `diaryx-org/typst`, public, as diaryx-org's Blitz
  fork already is. Its branch (say `twig`) is an upstream release tag plus a
  short stack of commits, each with a `PATCH.md` entry: what changed, why, and
  the upstream issue or PR that ends it.
- **Consumed from git, as the org's other forks are**: `typst = { git =
  "https://github.com/diaryx-org/typst", branch = "twig" }` in twig-export's
  own dependencies, not a `[patch]`. A dependency on a fork travels to
  consumers with twig-export; a `[patch]` does not, and every app would have
  to repeat it.
- **Rebased at each Typst release** onto the new tag, then absorbed as in
  step 5.
- **The cost**: twig-export can no longer be published to crates.io, because
  crates.io refuses git dependencies. It is not published today, and the org
  takes it from git, so the cost is small now. It grows if twig-export is
  ever meant to be published.

A vendored copy (`patches/typst-syntax/` and a `[patch.crates-io]`) is
acceptable as the first step, while one patch waits for upstream. It follows
the precedent of leaf's patched winit, and has the same weakness: each
consumer repeats the `[patch]`.

### 7. Licence

Typst's own crates (`typst`, `typst-library`, `typst-syntax`, ...) are
Apache-2.0. The libraries under them (`hayagriva`, `citationberg`, `wasmi`,
`hayro`) are MIT OR Apache-2.0. twig-export is MIT OR Apache-2.0, and linking Apache-2.0
code is already what it does. A fork must:

- keep Typst's `LICENSE` and any `NOTICE` file;
- mark each changed file as changed (Apache-2.0, section 4(b));
- stay Apache-2.0 for the forked code. Our own crates keep their dual
  licence.

A fork of a public repository is public, and Typst's code is already public,
so the org's public/private rule is not touched. No Diaryx code goes into the
fork.

## What this does not decide

- **Whether the content route replaces `.typ` as the PDF path.** The leaf
  proposal argues it. This proposal only says how to hold Typst if it does.
  With `.typ` alone, the seam is just the `World`, and an exact pin is
  enough with no canary tests beyond the golden ones.
- **The template's vocabulary.** Which words a template takes (a title page
  on or off, a dedication) belongs to the Diaryx proposal's "To settle".
- **Math.** Typst's math or leaf-math's SVG on the page is the leaf
  proposal's question 3.

## Order of work

1. **Pin with `=`** (step 1). One commit, no other change. It changes what a
   consumer resolves, so it carries a `Behavioural-change:` trailer.
2. **`typst_seam.rs`** with the `World` and, when the content builder lands,
   the span packing and the layout loop, with their canary tests (steps 2
   and 3). This is part of the leaf proposal's "content backend" item, not a
   separate project.
3. **The two upstream requests** (step 4), once Adam approves sending them.
4. **Measure a phone build** for the Diaryx proposal's binary-size question.
   Only that number can trigger the third fork condition.

## To settle

1. **Pin now, fork on a trigger.** Is this the line Adam wants, or is the
   fork wanted now, for control rather than need?
2. **Sending the two requests to Typst.** Both are small. Each is a public
   act, so each needs Adam's approval.
3. **Whether twig-export will ever be published to crates.io.** If it will,
   a fork is a larger cost, and the copies plus upstream requests matter more.
4. **The phone budget.** Is 47 MB acceptable in the iOS and Android apps,
   with Diaryx's own faces in place of typst-kit's? If not, how much must go?
