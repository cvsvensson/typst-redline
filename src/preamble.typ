// typst-redline: change markup. Replace this file with --preamble to restyle.
#let redline-style = (
  ins: rgb("#1565c0"),
  ins-fill: rgb("#d8eaff"),
  del: rgb("#c62828"),
  del-fill: rgb("#ffe3e3"),
  // Change bars sit at this distance from the left page edge.
  bar-x: 1.2cm,
  bar-width: 2pt,
)

#let redline-bar(c) = context {
  let x = here().position().x
  box(width: 0pt, place(
    dx: redline-style.bar-x - x,
    dy: -0.8em,
    rect(width: redline-style.bar-width, height: 1em, fill: c),
  ))
}

#let redline-blockbar(c, body) = context {
  let x = here().position().x
  block(
    stroke: (left: redline-style.bar-width + c),
    outset: (left: x - redline-style.bar-x),
    width: 100%,
    body,
  )
}

#let redline-strike-math(c, it) = context {
  let s = measure(it)
  box({
    it
    place(top + left, dy: s.height / 2, line(length: s.width, stroke: 0.7pt + c))
  })
}

#let redline-ins-rules(body) = {
  let st = redline-style
  show math.equation.where(block: false): it => box(fill: st.ins-fill, outset: (y: 2pt), it)
  show math.equation.where(block: true): it => redline-blockbar(st.ins, block(fill: st.ins-fill, width: 100%, it))
  show figure: it => redline-blockbar(st.ins, it)
  highlight(fill: st.ins-fill, body)
}

// Deleted content must not step equation or figure counters.
#let redline-del-rules(body) = {
  let st = redline-style
  set math.equation(numbering: none)
  set figure(numbering: none)
  show math.equation.where(block: false): it => redline-strike-math(st.del, it)
  show math.equation.where(block: true): it => redline-blockbar(st.del, block(fill: st.del-fill, width: 100%, it))
  show figure: it => redline-blockbar(st.del, it)
  text(fill: st.del, strike(stroke: 0.7pt + st.del, body))
}

#let redline-ins(body, start: false, end: false) = {
  if start { redline-bar(redline-style.ins) }
  redline-ins-rules(body)
  if end { redline-bar(redline-style.ins) }
}

#let redline-del(body, start: false, end: false) = {
  if start { redline-bar(redline-style.del) }
  redline-del-rules(body)
  if end { redline-bar(redline-style.del) }
}

#let redline-ins-block(body) = redline-blockbar(redline-style.ins, redline-ins-rules(body))
#let redline-del-block(body) = redline-blockbar(redline-style.del, redline-del-rules(body))
#let redline-del-heading(level, body) = redline-del-block(
  text(weight: "bold", size: if level == 1 { 1.4em } else { 1.2em }, body),
)

// entries: (level, target label or none, title, number of changes, deleted)
#let redline-summary(ins, del, entries) = {
  let st = redline-style
  block(text(size: 1.4em, weight: "bold")[Changes])
  [#text(fill: st.ins)[#ins insertions], #text(fill: st.del)[#del deletions].]
  if entries.len() == 0 { parbreak(); [No changes.] }
  for (level, target, title, n, deleted) in entries {
    let line = if deleted {
      text(fill: st.del)[#strike(title) (deleted)]
    } else if target == none {
      [#title (#n)]
    } else {
      let num = context {
        let el = query(target)
        if el.len() > 0 and el.first().func() == heading and el.first().numbering != none {
          numbering(el.first().numbering, ..counter(heading).at(target)) + h(0.5em)
        }
      }
      link(target)[#num#title] + [ (#n)]
    }
    block(above: 0.6em, below: 0.6em, pad(left: (level - 1) * 1.5em, line))
  }
  pagebreak()
  counter(page).update(1)
}
