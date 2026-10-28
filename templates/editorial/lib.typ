// Editorial: a magazine's page. Literata for the body, Atkinson Hyperlegible
// Next for the headings: a title over a rule, sections in tracked capitals.
// Justified on paper, with wide margins and spaced paragraphs; ragged when
// reflowed.
//
// A twig-export template: see classic/lib.typ for the function's arguments.
#let twig-template(
  body,
  layout: "paged",
  paper: "a4",
  width: auto,
  height: auto,
  size: 11pt,
  face: "serif",
  dark: false,
  meta: (:),
) = {
  let faces = (
    serif: ("Literata", "Noto Serif", "Libertinus Serif"),
    sans-serif: ("Atkinson Hyperlegible Next", "Noto Sans", "DejaVu Sans"),
    monospace: ("IBM Plex Mono", "DejaVu Sans Mono"),
    handwritten: ("Caveat", "Noto Serif"),
  )
  let body-face = faces.at(face, default: faces.serif)
  let display = faces.sans-serif
  let mono = faces.monospace
  let reflow = layout == "reflow"

  let ink = if dark { rgb("#e9e7e3") } else { rgb("#191919") }
  let quiet = if dark { rgb("#9d9a95") } else { rgb("#66625c") }
  let rule = if dark { rgb("#4b4945") } else { rgb("#c9c5bd") }
  let tint = if dark { rgb("#272624") } else { rgb("#f5f3ef") }
  let ground = if dark { rgb("#1a1918") } else { white }
  let accent = if dark { rgb("#e07a5f") } else { rgb("#b23a1d") }

  let measure = size * 35
  let page-args = if reflow {
    let side = calc.max(size * 1.5, (width - measure) / 2)
    (width: width, height: auto, margin: (x: side, top: size * 2.2, bottom: size * 3))
  } else if width != auto {
    (width: width, height: height, margin: (x: 2.8cm, top: 2.6cm, bottom: 2.8cm))
  } else {
    (paper: paper, margin: (x: 2.8cm, top: 2.6cm, bottom: 2.8cm))
  }
  set page(
    ..page-args,
    // Reflowed, the page is the view's own ground, which the host draws
    // under it; the ink still follows `dark`.
    fill: if reflow { none } else { ground },
    numbering: if reflow { none } else { "1" },
    number-align: right,
  )
  set text(font: body-face, size: size, fill: ink, hyphenate: not reflow)
  set par(
    justify: not reflow,
    leading: 0.7em,
    // A paragraph's spacing stands in for the leading after its last line.
    spacing: 1.4em,
    overflow-wrap: "break-word",
  )

  show heading: set text(font: display, weight: "bold")
  show heading.where(level: 1): it => block(
    above: 1.4em, below: 1.2em, width: 100%,
    stroke: (bottom: 1.2pt + ink), inset: (bottom: 0.45em),
    text(size: 2em, weight: "bold", it.body),
  )
  show heading.where(level: 2): it => block(
    above: 1.9em, below: 0.8em,
    text(size: 0.82em, weight: "bold", tracking: 0.12em, fill: accent, upper(it.body)),
  )
  show heading.where(level: 3): set text(size: 1.05em)
  show heading.where(level: 3): set block(above: 1.4em, below: 0.6em)
  show heading.where(level: 4): set text(size: 0.95em, fill: quiet)

  show raw: set text(font: mono, size: 0.86em)
  show raw.where(block: true): it => block(
    width: 100%, fill: tint, inset: (x: 1em, y: 0.8em), it,
  )
  show quote.where(block: true): it => block(
    inset: (x: 1.5em, y: 0.3em),
    text(size: 1.15em, style: "italic", it.body),
  )
  show link: it => text(fill: accent, it)
  set line(stroke: 0.8pt + rule)
  set table(stroke: (x, y) => if y == 0 { (bottom: 0.8pt + ink) } else { (bottom: 0.4pt + rule) },
    inset: (x: 0.6em, y: 0.5em))
  set list(marker: text(fill: accent)[•], indent: 0.3em)
  set enum(indent: 0.3em)
  set footnote.entry(separator: line(length: 20%, stroke: 0.8pt + accent))

  body
}
