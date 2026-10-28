// Classic: a book's page. Literata throughout. On paper, justified and
// hyphenated, its paragraphs marked by an indent; reflowed for a screen,
// ragged, its paragraphs marked by space. Headings in the body's face, set
// apart by size and an italic.
//
// A twig-export template: one function, `twig-template`, of the body and how
// it is set. `layout` is "paged" or "reflow"; a reflowed page is `width` wide
// and as tall as the text. Sizes are in em of `size`, the body's, so the
// reader's text size scales every style. `face` is the document's face by
// word (serif, sans-serif, monospace, handwritten).
#let twig-template(
  body,
  layout: "paged",
  paper: "a4",
  width: auto,
  height: auto,
  size: 11pt,
  face: "serif",
  dark: false,
) = {
  let faces = (
    serif: ("Literata", "Noto Serif", "Libertinus Serif"),
    sans-serif: ("Atkinson Hyperlegible Next", "Noto Sans", "DejaVu Sans"),
    monospace: ("IBM Plex Mono", "DejaVu Sans Mono"),
    handwritten: ("Caveat", "Noto Serif"),
  )
  let body-face = faces.at(face, default: faces.serif)
  let mono = faces.monospace
  let reflow = layout == "reflow"

  let ink = if dark { rgb("#e8e6e1") } else { rgb("#1b1b1b") }
  let quiet = if dark { rgb("#a09d97") } else { rgb("#6b6862") }
  let rule = if dark { rgb("#4a4846") } else { rgb("#cfccc6") }
  let tint = if dark { rgb("#2a2928") } else { rgb("#f4f2ee") }
  let ground = if dark { rgb("#1c1b1a") } else { white }
  let accent = if dark { rgb("#d9a25f") } else { rgb("#8a4b14") }

  // The page: a sheet, or one page as wide as the view and as tall as the
  // text, its column held to a book's measure.
  let measure = size * 33
  let page-args = if reflow {
    let side = calc.max(size * 1.5, (width - measure) / 2)
    (width: width, height: auto, margin: (x: side, top: size * 2, bottom: size * 3))
  } else if width != auto {
    (width: width, height: height, margin: (inside: 2.4cm, outside: 2cm, y: 2.4cm))
  } else {
    (paper: paper, margin: (inside: 2.4cm, outside: 2cm, y: 2.4cm))
  }
  set page(
    ..page-args,
    // Reflowed, the page is the view's own ground, which the host draws
    // under it; the ink still follows `dark`.
    fill: if reflow { none } else { ground },
    numbering: if reflow { none } else { "1" },
    number-align: center,
  )
  set text(font: body-face, size: size, fill: ink, hyphenate: not reflow)
  set par(
    justify: not reflow,
    leading: 0.68em,
    spacing: if reflow { 1.1em } else { 0.68em },
    first-line-indent: if reflow { 0em } else { 1.3em },
    overflow-wrap: "break-word",
  )

  show heading: set text(font: body-face, weight: "regular")
  show heading: set block(above: 1.7em, below: 0.85em)
  show heading.where(level: 1): set text(size: 1.9em, weight: "medium")
  show heading.where(level: 2): set text(size: 1.4em)
  show heading.where(level: 3): set text(size: 1.15em, style: "italic")
  show heading.where(level: 4): set text(size: 1em, style: "italic")

  show raw: set text(font: mono, size: 0.88em)
  show raw.where(block: true): it => block(
    width: 100%, fill: tint, inset: (x: 0.9em, y: 0.75em), radius: 2pt, it,
  )
  show quote.where(block: true): it => block(
    inset: (left: 1.2em), stroke: (left: 0.6pt + rule),
    text(style: "italic", fill: quiet, it.body),
  )
  show link: it => underline(offset: 2pt, stroke: 0.4pt + accent, it)
  set line(stroke: 0.5pt + rule)
  set table(stroke: 0.5pt + rule, inset: (x: 0.6em, y: 0.45em))
  set list(marker: [–], indent: 0.4em)
  set enum(indent: 0.4em)
  set footnote.entry(separator: line(length: 25%, stroke: 0.5pt + rule))

  body
}
