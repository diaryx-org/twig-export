---
title: Typst is forked, and kept under Markdown
description: Typst as a thin layer means the author never writes Typst and the only Typst code is a template for the procedural part of a page — a shape of use, not a cut of the code — so twig-export takes Typst from diaryx-org's fork, an upstream release plus two small patches (a public range span and a compile entry point that takes content), rebased at each Typst release, with every patch listed and tested, and asks upstream for both later
status: accepted
created: 2026-10-26
updated: 2026-10-26
part_of: '[Proposals](/docs/proposals/proposals.md)'
---

# Typst is forked, and kept under Markdown

## Status

`accepted`, 2026-10-26. Argued the same day from twig-export `main` at
`bf145b7` (twig 6, fig 7, Typst 0.15.1). The first draft proposed an exact
pin from crates.io and a fork only on a trigger. Adam chose the fork now
(**Decisions**), and this version records that.

Built on this branch:

- **The fork**: [`diaryx-org/typst`](https://github.com/diaryx-org/typst),
  branch `twig` at `d2a62e1`: upstream `v0.15.1` (`9dfd3a0`) plus two
  patches, listed in its `PATCH.md`, and a commit to that file.
- **twig-export takes every Typst crate from it**, by git.
- **A test** in `src/typst_world.rs` (`fork_tests`) uses both patches the way
  the content route will.

The binary sizes below were measured on this branch, on an Apple Silicon
Mac.

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

## Decisions

Adam, 2026-10-26, after the first draft:

- **Fork now, for the two patches.** "I think those two requests are worth
  forking for." The first draft would have copied Typst's code instead.
- **Upstream requests come later.** The fork does not wait for them.
- **twig-export is not published to crates.io.** So the one real cost of a
  git dependency (crates.io refuses a crate that has one) does not apply.

## The short answer

**"Thin" is a shape of use, not a cut of the code.** The author writes
Markdown or djot with directives. twig-export builds the body as Typst
content in Rust, so no Typst markup is made for the words. The only Typst
code is a *template*: the contents page, the running heads, the page numbers,
the heading numbers. Typst's engine stays whole, because the engine is what
makes the page exact. Typst's language stays too, because a template needs
`context`, `query` and `outline`. Only the author stops seeing it.

**A fork, kept small.** The content route needs two things that Typst 0.15.1
keeps private. Without a fork, twig-export would copy Typst's code for both,
and a copy goes quietly wrong when Typst changes what it copied. With a fork,
each is a one-place patch that the compiler checks:

| | What it is | Cost per Typst release | Verdict |
| --- | --- | --- | --- |
| **Fork** | `diaryx-org/typst`, branch `twig`: an upstream tag plus a short list of patches | Rebase the patches onto the new tag; run Typst's tests and ours | **Yes** (Adam, 2026-10-26) |
| Exact pin, from crates.io | `typst = "=0.15.1"`, with the two items copied into twig-export | Bump; fix what moved; canary tests must catch what the compiler cannot | The first draft's choice |
| Pin + vendored patches | One or two Typst crates copied into the repo and swapped in with `[patch.crates-io]` | As the fork, and each consumer must repeat the `[patch]` | No |

The fork is not a cut. It does not remove the parts twig-export never uses
(**What a cut would save**). That stays a later option, for a phone.

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
  template. Today `pdf.rs` must quote the generated source and tell the user
  to read `-o out.typ`.
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

Measured on this branch, before the fork (the fork changes no sizes):
`cargo bloat --release --no-default-features --features pdf --bin
twig-export --crates`, then `strip`.

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

The fonts are a larger lever, and they need no patch. They are typst-kit's
`embedded-fonts` feature. The Diaryx proposal already wants one bundled face
for each generic family, so Diaryx would swap typst-kit's set for its own.

So a cut saves about an eighth of the binary. That matters on a phone, if at
all. On the Mac and the CLI it does not. The direct imports are few (two
files for `hayagriva`, one for `wasmi`, two for `hayro`), but the elements
they back are named in layout, realize, PDF and HTML export, so a cut is a
much larger patch than the two below, and it would cost more at every
rebase. It waits for a phone measurement.

## What twig-export reaches into

The content route uses Typst as a Rust library, not through its text format.
The leaf spike's `content.rs` is the complete list of what it touches:

| What | From | Public in 0.15.1? | In the fork |
| --- | --- | --- | --- |
| `World`, `Library::builder`, `LazyHash`, `Source`, `FileId` | `typst`, `typst-syntax` | Yes | Unchanged |
| Elements: `HeadingElem`, `TextElem`, `StrongElem`, `EmphElem`, `ListElem`, `FootnoteElem`, ... and `Content::sequence`, `.spanned()` | `typst-library` | Yes | Unchanged |
| The template's function: the module's scope, `Func::call` | `typst-library` | Yes | Unchanged |
| The layout loop: evaluate the main file, then lay out again until introspection settles | `typst` (`compile_impl`) | **No**. The spike copied its 60 lines, and `typst_eval::eval`, `Engine`, `Constraint` and `MAX_ITERS` with them | **`typst::compile_with`** |
| A span that is a block and an offset | `typst-syntax` (`Span::from_range`) | **No**, `pub(crate)`. The spike packed its bits by hand through `Span::from_raw` | **Public** |
| A glyph's span, for click to source | `typst-library` (`FrameItem::Text`, `Glyph::span`) | Yes | Unchanged |
| PDF: `typst_pdf::pdf` | `typst-pdf` | Yes | Unchanged |

"Public" is not "stable". Typst is 0.x, and each minor release may move any
of these. The spike's world code already shows it: `RootedPath` and
`VirtualRoot` are new in 0.15. But a moved public item fails at compile time,
in twig-export, where it is fixed. The two private items were the ones that
could go wrong without an error, and the fork turns both into calls.

## The fork

### What it holds

Two patches on `twig`, over `v0.15.1` (a third commit changes only
`PATCH.md`):

1. **`Span::from_range` is public** (`crates/typst-syntax/src/span.rs`). One
   word, `pub(crate)` to `pub`. `DiagSpan::from_range` is already public, for
   diagnostics only, so the idea is not new to Typst.
2. **`typst::compile_with`** (`crates/typst/src/lib.rs`):

   ```rust
   pub fn compile_with<T: Output>(
       world: &dyn World,
       body: &dyn Fn(&mut Engine, &Module) -> SourceResult<Content>,
   ) -> Warned<SourceResult<T>>
   ```

   It evaluates the main file as `compile` does, then lays out what `body`
   builds from the module, once per layout iteration, with that iteration's
   engine. twig-export's main file is the template; `body` calls its `book`
   function with the content built from the twig tree. The content never
   goes into the `World` or the `Library`, which the leaf spike found makes
   every keystroke a cold compile. `compile_impl` now takes the body as a
   callback, and `compile` and `trace` pass one that returns the module's
   content, so they behave as before.

Each changed place is marked in the source, and `PATCH.md` at the fork's
root says what each patch does, why, and what ends it.

### What it may hold

A patch goes on `twig` only if the content route needs it, or a fix is
needed before Typst releases it. Each gets a `PATCH.md` entry and a test
here or in Typst's suite. A cut of the unused parts is not on the list until
a phone build shows it is needed.

### How twig-export takes it

Every Typst crate comes from the fork, by branch, as the org spells its
other git dependencies:

```toml
typst = { git = "https://github.com/diaryx-org/typst", branch = "twig", optional = true }
typst-pdf = { git = "https://github.com/diaryx-org/typst", branch = "twig", optional = true }
# ... typst-kit, typst-layout, typst-html the same way
```

`Cargo.lock` holds the commit. `typst-assets` (the fonts and data) is a
separate upstream crate and still comes from crates.io.

A git dependency travels to consumers with twig-export, so leaf and Diaryx
get the fork without a `[patch]`. A consumer that names a Typst crate itself
(leaf's page view will, for frames or `typst-render`) must name the same git
source and branch. If it names crates.io instead, its graph has two
Typsts, which is two engines: the thing the leaf proposal exists to avoid.

### What tests it

- **In twig-export**, `fork_tests` in `src/typst_world.rs` evaluates a
  template, calls `book` through `compile_with` with a `TextElem` that has a
  `Span::from_range` into a Markdown file Typst never reads, and checks that
  every laid-out glyph carries that span. If a rebase drops either patch, the
  test does not compile. If `compile_with` stops laying out the body, or a
  glyph loses its span, it fails.
- **In the fork**, Typst's own suite covers `compile`, which now runs through
  the changed `compile_impl`. Run for the patch, filtered to the tests that
  lean on the layout loop (`outline footnote counter query heading`): 182
  passed, 0 failed. The full suite was not run.
- **Later, with the content backend**: the leaf proposal's test, the corpus
  set through the content builder and through `typst_emit.rs`, must give the
  same pages.

### How a Typst release is absorbed

1. In the fork: fetch the new tag, rebase `twig` onto it, and update
   `PATCH.md`'s base. Drop a patch upstream now has.
2. Run Typst's suite on the result, at least the filter above.
3. Push `twig`. A rebase rewrites the branch, so push with
   `--force-with-lease`. A consumer's `Cargo.lock` still holds the old
   commit, which stays reachable only while something refers to it, so tag
   the old head first (`twig-0.15.1`) to keep every locked commit
   fetchable.
4. In twig-export: `cargo update -p typst`, build, fix what moved, run the
   tests, review a PDF of `examples/report.md` by eye (a layout change is not
   an error), and merge.
5. In leaf and Diaryx: `cargo update -p twig-export -p typst` in the same
   session, so no app is on two Typsts.

Typst's minor releases have come a few months apart. A bump is a planned
afternoon, and nothing forces one except a fix we need.

### Upstream, later

Both patches are small and general, so each is worth offering to
`typst/typst`, first as an issue. When upstream has an equivalent, the patch
leaves `twig`. If both leave, twig-export can return to crates.io Typst at
an exact version, and the fork can be archived. Sending either publishes, so
each waits for Adam.

### Licence

Typst's own crates (`typst`, `typst-library`, `typst-syntax`, ...) are
Apache-2.0. The libraries under them (`hayagriva`, `citationberg`, `wasmi`,
`hayro`) are MIT OR Apache-2.0. twig-export is MIT OR Apache-2.0, and linking
Apache-2.0 code is already what it does. The fork:

- keeps Typst's `LICENSE` unchanged;
- marks each changed place in the source as changed by diaryx-org, and lists
  the changes in `PATCH.md` (Apache-2.0, section 4(b));
- stays Apache-2.0. Our own crates keep their dual licence.

The fork is public, as Typst is, so the org's public/private rule is not
touched. No Diaryx code goes into it.

## What this does not decide

- **Whether the content route replaces `.typ` as the PDF path.** The leaf
  proposal argues it. Today's `.typ` path runs on the fork unchanged.
- **The template's vocabulary.** Which words a template takes (a title page
  on or off, a dedication) belongs to the Diaryx proposal's "To settle".
- **Math.** Typst's math or leaf-math's SVG on the page is the leaf
  proposal's question 3.

## Order of work

1. ~~**The fork**, with the two patches and `PATCH.md`.~~ Done, 2026-10-26.
2. ~~**twig-export on the fork**, with `fork_tests`.~~ Done on this branch.
3. **The content backend**, which calls `compile_with` and
   `Span::from_range` through one module (`typst_world.rs` or a
   `typst_seam.rs` beside it). This is the leaf proposal's "content backend"
   item.
4. **leaf's spike and page view on the fork**: replace the copied layout
   loop and the hand-packed span, and name the fork for any Typst crate
   leaf links itself.
5. **Measure a phone build** for the Diaryx proposal's binary-size question.
   Only that number can put a cut on the fork.
6. **The two upstream requests**, when Adam approves sending them.

## To settle

1. **The org's dependency list.** The org `AGENTS.md` lists the crates taken
   from git. The fork is a new one, consumed by branch like the others, and
   is not on the list. That file is the org's to change.
2. **The phone budget.** Is 47 MB acceptable in the iOS and Android apps,
   with Diaryx's own faces in place of typst-kit's? If not, how much must go?
3. **When to send the upstream requests.**
