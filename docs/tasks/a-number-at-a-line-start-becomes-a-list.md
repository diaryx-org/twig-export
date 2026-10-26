---
title: A number at the start of a line becomes a list item
part_of: '[Tasks](/docs/tasks/tasks.md)'
status: open
created: 2026-10-26
---

# A number at the start of a line becomes a list item

`escape` in `src/typst_emit.rs` puts a backslash before every character
that is special in Typst markup, and escapes `-`, `+`, `/` and `=`
unconditionally because "a paragraph reflowed by a later edit can put any
run at the start of a line". It misses Typst's other line-start marker: a
run of digits and a `.` (`2024.`) opens a numbered list item. A soft break
is emitted as a newline, so prose that carries such a run onto a new line
comes out as a list.

## Repro

```sh
printf 'It began in\n2024. We left.\n' > n.md
twig-export n.md            # …It began in\n2024. We left.
twig-export n.md -o n.pdf
pdftotext -layout n.pdf -   # two lines: a paragraph, then item 2024
```

In Markdown that is one paragraph: only a `1.` may interrupt one, so twig
reads `2024. We left.` as text after a soft break. The same text with
`the year` before `2024.` renders as one line, as it should.

A paragraph that opens with an escaped number does the same:

```sh
printf 'Intro.\n\n2024\\. Was the year.\n' > p.md
twig-export p.md -o p.pdf   # `2024.` is set as a list marker, its gap twice a word space
```

## Done when

Text whose line starts with digits and a `.`, after a soft break or at the
start of a paragraph, renders as text in the `.typ` and the PDF, and a test
in `tests/` holds it. Escaping the `.` (Typst reads `\` before ASCII
punctuation as that punctuation) is the shape `escape` already takes for
the other markers.
