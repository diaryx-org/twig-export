---
title: An empty list in the frontmatter fails the export
part_of: '[Tasks](/docs/tasks/tasks.md)'
status: done
created: 2026-11-09
updated: 2026-11-09
description: '`typst_value` writes a JSON array as `(a, b,)`, so an empty one is `(,)`, which Typst refuses; a document whose frontmatter holds `tags: []` and reaches a template through `template-meta` fails to compile'
---

# An empty list in the frontmatter fails the export

`typst_value` in `src/typst_emit.rs` turns frontmatter into the Typst value
an installed template gets as `meta`. It writes an array as its items joined
by `, ` with a comma after them, `(a, b,)`. The trailing comma keeps a
one-item array from reading as parentheses around a value. With no items it
writes `(,)`, which is not Typst: an empty array is `()`. A document whose
frontmatter holds an empty list, at the top level or inside a mapping, fails
to typeset whenever `template-meta` passes that key to the template.

## Repro

At eb5f284, with the `local/plain` package from `tests/packages.rs`
(`#let twig-template(body, meta: (:), ..rest) = { … }`), `template-meta =
["*"]`, and this document:

```markdown
---
tags: []
---

Body.
```

```console
typst failed to compile the generated document:
  - unexpected comma
    at: ,
```

The generated source holds `meta: ("tags": (,))`. `tags: [a]` sets, and so
does a document with no `tags`.

## What it needs

An empty array written as `()`, as the empty mapping beside it is already
written as `(:)`. A test beside
`a_template_on_disk_is_found_and_given_the_listed_frontmatter` that sets a
document with an empty list, at the top level and nested in a mapping, and
reads its length in the template.

## Done when

A document with `tags: []` in its frontmatter sets under a template that
reads `meta`, and the template sees an empty array.

## Resolved

`typst_value` writes an array with no items as `()`, the way it already
writes an empty mapping as `(:)`; an array with items keeps its trailing
comma. `an_empty_list_in_the_frontmatter_reaches_the_template_as_an_empty_array`
in `tests/packages.rs` sets a document with `tags: []` at the top level and
inside a mapping, and its template reads each as an array of length 0.
