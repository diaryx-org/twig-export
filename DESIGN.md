---
part_of: '[Twig Export](/README.md)'
---
# Twig Export — design notes

Rationale for the decisions the source comments refer to. Twig's own
`DESIGN.md` is the model for this file, and several conventions are carried
over deliberately.

---

## The three candidate backends

| | Typography | Config surface | Runtime dependency |
|---|---|---|---|
| **Typst** | Excellent — Knuth-Plass, hyphenation, footnotes, math | `set`/`show` rules are already selector→styling | Linked in-process; none external |
| Handrolled PDF | Whatever gets written | Must be invented *and* implemented | None |
| HTML + CSS paged media | Weaker; footnotes and math especially | CSS, which users know | A browser or Python |

Typst won on the second column more than the first. Rendering is a solved
problem either way; the config requirement is what a handrolled backend makes
unbounded, because the vocabulary would have to be both designed and given a
layout engine to mean anything.

HTML + CSS is a real option and stays viable — Twig's HTML renderer is already
complete, so that backend is a sibling of `typst_emit.rs` and needs no changes
anywhere else. That possibility is the reason the style vocabulary is neutral.

**Handrolling would be right** under one specific brief: a hard requirement of
no external toolchain *and* an accepted scope of Latin-only text, one embedded
font, and greedy line breaking. That is a receipt printer, not a document
renderer, and it is not what this is.

---

## Why `.typ` is the target and PDF is a step

Everything this crate decides is expressed in the generated Typst. Making that
the artifact means:

- The whole mapping is a text-to-text function, so `tests/golden/report.typ`
  pins every rendering decision at once and a regression is a readable diff.
- A rendering bug can be *read*, and `-o out.typ` is the first debugging step
  for any surprise.
- `tests/pdf.rs` then only has to answer a different question — is the emitted
  markup *valid* — which the golden test structurally cannot catch.

---

## The twig seam, and what it cost

This is the part that was not obvious from the outside, and it is why
`src/doc.rs` is larger than "deserialize some JSON".

Neither of Twig's read surfaces is sufficient alone:

- `Document::nodes()` carries the `NodeId`s that `Document::query()` returns —
  needed to attach style rules to nodes — but its payload projection lacks a
  code block's `lang`, an ordered list's `numbering`/`start`, a task item's
  `checked`, and a metadata island's `lang`.
- `Document::ast_json()` carries every payload and no ids.

So we walk both and zip them, matching on kind **and** span at every step.

### The zip is structural, not positional

The obvious implementation — "both are document-order walks, pair them by
index" — is wrong, and Twig says so on any document with a footnote.

`nodes()` dumps the whole **arena**; `ast_json()` walks the **tree** from the
`doc` root. A footnote *definition* is parsed into a node with `parent == None`
that is not among the root's descendants. On `examples/report.md` the arena
holds 116 nodes and the tree holds 113.

The tree half of the zip therefore follows the flat array's own
`first_child`/`next_sibling` links from the root, which reproduces exactly the
sequence `ast_json` emits regardless of arena layout. Detached subtrees are
ingested separately so footnote bodies still render and still match selectors.

This was found by the consistency check rather than in review, which is the
argument for keeping the check: it is cheap, and it converts a whole class of
silent style misattribution into one error naming the diverging node.

### Reading through spans

A footnote definition's **label** is in neither projection. It is recovered by
slicing the source with the node's span — idiomatic for Twig, whose
`DESIGN.md` says "the correctness bar is the real source" and whose every node
carries a byte span into the true input.

### Why not reimplement the selector grammar

The config surface is *defined* in terms of Twig's selectors. A second
implementation would mean a config file that silently means different things
to `twig query` and to this crate. Threading ids across is the cost of having
exactly one implementation of the grammar, and it is worth paying.

---

## The config schema is a layer, not a pile of checks

The config file *is* the product — "which directives and metadata drive which
styling" is the whole feature — so config error quality is not a side concern.

The first implementation validated after deserialization: serde produced typed
structs, then a hand-rolled pass walked them checking lengths and colours and
reporting `rule #2`. That works, and it is strictly worse than validating the
`fig::Value` tree first, which is what `fig-schema` is for:

| | Hand-rolled, post-serde | `fig-schema`, pre-serde |
|---|---|---|
| Location | `rule #2` | `rule[1].box.fill` |
| Misspelled property | serde `unknown field` | "did you mean `weight`?" |
| Misspelled value | serde enum error | "did you mean `semibold`?" |
| Unknown-but-plausible value | must hard-fail | can warn and pass through |
| Reporting | first error only | all of them at once |

The last two rows are the ones that changed behaviour rather than wording.

**Open versus closed vocabularies.** Serde can only hard-fail, so anything
Typst might plausibly extend had to be typed as a bare `String` with no
checking at all — `paper`, `font`. A schema constraint can be *open*: an
unrecognised paper name warns, and the value still reaches Typst, which knows
more paper names than any table here would stay current with. `weight`,
`align` and `break-*` are genuinely closed and reject.

**Near-miss suggestions** come from `fig-schema`'s edit-distance search over
the vocabulary, and they apply to property *names* as well as values —
`validate_enum` run over a map's keys against a list of the legal ones. That
list is a second copy of the field names in `style.rs`, so `tests/schema.rs`
serializes a fully-populated `Style` and compares key sets: adding a field
without updating the schema fails a test rather than silently degrading to
"unknown key".

`fig_schema::Presentation` (icons, tints) is deliberately unused. It carries
display hints for an editor UI over a config file, and this is a batch
renderer with no such surface — but if a config editor ever wants one, the
hints belong in these same rules.

The cost is that the config is parsed twice, once structurally and once
through serde, because fig has no `from_value`. A style config is a hundred
lines; the diagnostics are worth the microseconds.

---

## Precedence is source order

Rules apply top to bottom, each overlaying the last property by property.
There is deliberately no CSS-style specificity calculation.

Specificity's payoff — order-independent stylesheets that can be concatenated
— is a payoff for *distributed* authorship of huge sheets. A document style
config is one file, usually under a hundred lines, written by one person.
Source order is something you can see; specificity is something you have to
compute.

Overlay is per-property, which is what makes the two-rule idiom work: a
`.warn` note restates only its fill and keeps the base note's stroke, padding
and radius.

---

## Inheritance is Typst's job

A style attaches to the node its rule matched and to nothing else. Typst
already inherits text properties through content nesting, so a `font` set on a
container genuinely does reach the paragraphs inside it — but that is Typst's
inheritance at layout time, not a cascade computed here.

Precomputing inherited values would mean resolving relative units (`1.2em` of
what?) against a font size unknown until layout, which is precisely the job
being delegated.

---

## The escape hatch is deliberately visible

A neutral vocabulary covering *everything* is just Typst with worse
ergonomics. A config where the escape hatch is invisible is one that cannot be
ported. So `[rule.raw]` names the backend at the point of use, and reading a
config makes obvious which parts would survive a backend swap.

Within it, `before`/`after`/`preamble` **concatenate** across rules while
`wrap` **replaces** — two wrappers would nest ambiguously, and the later, more
specific rule is the one that meant it.

---

## Two emitter decisions worth recording

**Code blocks are `#raw(...)` calls, not ``` fences.** A fence's content is
column-sensitive and block styling indents whatever it wraps, so a styled code
block inside a `#block[…]` would silently gain leading whitespace on every
line. A string argument is immune to surrounding indentation and sidesteps
fence-length arithmetic against backticks in the content.

**Checkboxes are drawn, not spelled.** `☐`/`☑` are absent from most serif text
fonts, so an unchecked box renders as tofu while the checked one silently
falls back to another face. Drawing them is font-independent.

**Unknown kinds render their children rather than vanishing.** Twig's kind
vocabulary is explicitly extensible (`AST-KINDS.md`), so "unstyled but
present" is the right failure mode for a kind this backend has not learned.

---

## Fonts are embedded-first

`typst-kit`'s embedded set is registered before system fonts, so a document
renders identically on a machine with no fonts installed and one with a
thousand. `World::today` returns `None`, making `datetime.today()` an error
rather than silently baking a date into a document meant to render identically
tomorrow.
