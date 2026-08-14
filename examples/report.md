---
title: Quarterly Field Notes
author: A. Harris
pdf-paper: a4
pdf-toc: 2
---

# Overview

A paragraph with *emphasis*, **strong**, `code`, and a [link](https://example.com).
Special characters that must survive escaping: 100% of $5 — a_b, c*d, #e.

:::note
An admonition. Styled by `container[name=note]` in the config,
with no raw Typst anywhere.
:::

:::note{.warn}
A *warning* variant. The config's second rule overlays only the fill,
so this keeps the base note's stroke and padding.
:::

:::internal
This paragraph is dropped entirely — `hidden = true`. It is the PDF
analogue of `twig filter --drop`.
:::

## Structure

1. First
2. Second, with a nested list:
   - alpha
   - beta

- [ ] an unchecked task
- [x] a checked task

> A block quote.
> It spans two lines.

| Format | Parses | Writes |
|:-------|:------:|-------:|
| Djot   | yes    | yes    |
| PDF    | no     | yes    |

```zig
const std = @import("std");
```

Twig
: A document AST library.

Fig
: A config file library.

A footnote reference[^1].

[^1]: The footnote body, pulled in at the reference site.

## Appendix

This heading starts a new page, via `break-before = "page"`.
