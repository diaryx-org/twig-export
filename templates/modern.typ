// Modern: a note's page, as Notes sets one. Atkinson Hyperlegible Next,
// ragged on paper and on screen, its paragraphs marked by space. Headings in
// the same face, set apart by weight; links in the accent, not underlined.
//
// A twig-export template: see classic.typ for the function's arguments.
#let twig-template(
  body,
  layout: "paged",
  paper: "a4",
  width: auto,
  height: auto,
  size: 11pt,
  face: "sans-serif",
  dark: false,
) = {
  let faces = (
    serif: ("Literata", "Noto Serif", "Libertinus Serif"),
    sans-serif: ("Atkinson Hyperlegible Next", "Noto Sans", "DejaVu Sans"),
    monospace: ("IBM Plex Mono", "DejaVu Sans Mono"),
    handwritten: ("Caveat", "Noto Sans"),
  )
  let body-face = faces.at(face, default: faces.sans-serif)
  let mono = faces.monospace
  let reflow = layout == "reflow"

  let ink = if dark { rgb("#ececef") } else { rgb("#1d1d1f") }
  let quiet = if dark { rgb("#a1a1a6") } else { rgb("#6e6e73") }
  let rule = if dark { rgb("#3a3a3c") } else { rgb("#d2d2d7") }
  let tint = if dark { rgb("#2c2c2e") } else { rgb("#f2f2f5") }
  let ground = if dark { rgb("#1c1c1e") } else { white }
  let accent = if dark { rgb("#4ea1ff") } else { rgb("#0a66d6") }

  let measure = size * 38
  let page-args = if reflow {
    let side = calc.max(size * 1.4, (width - measure) / 2)
    (width: width, height: auto, margin: (x: side, top: size * 1.8, bottom: size * 3))
  } else if width != auto {
    (width: width, height: height, margin: (x: 2.2cm, y: 2.2cm))
  } else {
    (paper: paper, margin: (x: 2.2cm, y: 2.2cm))
  }
  set page(
    ..page-args,
    // Reflowed, the page is the view's own ground, which the host draws
    // under it; the ink still follows `dark`.
    fill: if reflow { none } else { ground },
    numbering: if reflow { none } else { "1 / 1" },
    number-align: right,
  )
  set text(font: body-face, size: size, fill: ink, hyphenate: false)
  set par(
    justify: false,
    leading: 0.78em,
    // A paragraph's spacing stands in for the leading after its last line.
    spacing: 1.5em,
    overflow-wrap: "break-word",
  )

  show heading: set text(font: body-face, weight: "bold")
  show heading: set block(above: 1.6em, below: 0.7em)
  show heading.where(level: 1): set text(size: 1.75em)
  show heading.where(level: 2): set text(size: 1.35em)
  show heading.where(level: 3): set text(size: 1.12em)
  show heading.where(level: 4): set text(size: 1em, fill: quiet)

  show raw: set text(font: mono, size: 0.9em)
  show raw.where(block: false): it => box(
    fill: tint, inset: (x: 0.25em), outset: (y: 0.2em), radius: 3pt, it,
  )
  show raw.where(block: true): it => block(
    width: 100%, fill: tint, inset: (x: 1em, y: 0.85em), radius: 6pt, it,
  )
  show quote.where(block: true): it => block(
    inset: (left: 1em, y: 0.15em), stroke: (left: 2.5pt + accent),
    text(fill: quiet, it.body),
  )
  show link: set text(fill: accent)
  set line(stroke: 0.6pt + rule)
  set table(stroke: 0.6pt + rule, inset: (x: 0.65em, y: 0.5em))
  set list(marker: text(fill: quiet)[•], indent: 0.2em)
  set enum(indent: 0.2em)
  set footnote.entry(separator: line(length: 30%, stroke: 0.6pt + rule))

  body
}
