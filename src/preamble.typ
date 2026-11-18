// typst-redline: change markup. Replace this file with --preamble to restyle.
#let redline-style = (
  ins: rgb("#1565c0"),
  ins-fill: rgb("#d8eaff"),
  del: rgb("#c62828"),
  // Change bars sit this far outside the text area, at most half the margin.
  bar-gap: 0.5cm,
  bar-width: 2pt,
)

// Each mark is bracketed by two of these; `redline-bars` draws a bar in the
// margin next to every line between them.
#let redline-tags = (
  ins: [#metadata("ins")<redline-tag>],
  del: [#metadata("del")<redline-tag>],
  end: [#metadata("end")<redline-tag>],
)

// The page foreground: one query per page instead of introspection per mark,
// so that marks cost no extra layout.
#let redline-bars = context {
  let st = redline-style
  let tags = query(<redline-tag>)
  let n = calc.quo(tags.len(), 2)
  if n == 0 { return }
  let pg = here().page()
  let em = 1em.to-absolute()
  let (w, h) = (page.width, page.height)
  let m = page.margin
  let even = calc.even(pg)
  let side(keys, full) = {
    let v = if type(m) == dictionary { keys.map(k => m.at(k, default: none)).find(v => v != none) } else { m }
    if v == none or v == auto { 2.5 / 21 * calc.min(w, h) } else if type(v) == length { v.to-absolute() } else if (
      type(v) == ratio
    ) { v * full } else { v.length.to-absolute() + v.ratio * full }
  }
  let left = side(if even { ("left", "outside") } else { ("left", "inside") }, w)
  let right = side(if even { ("right", "inside") } else { ("right", "outside") }, w)
  let (top, bottom) = (side(("top",), h), h - side(("bottom",), h))
  // Tags come in document order, so the first pair ending on this page is
  // found by bisection.
  let (lo, hi) = (0, n)
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    if tags.at(2 * mid + 1).location().page() < pg { lo = mid + 1 } else { hi = mid }
  }
  let i = lo
  while i < n {
    let (s, e) = (tags.at(2 * i).location().position(), tags.at(2 * i + 1).location().position())
    if s.page > pg { break }
    let y0 = if s.page == pg { s.y - 0.8 * em } else { top }
    let y1 = if e.page == pg { e.y + 0.3 * em } else { bottom }
    let x = if page.columns > 1 and s.x > w / 2 { w - right + calc.min(st.bar-gap, right / 2) } else {
      left - calc.min(st.bar-gap, left / 2)
    }
    let c = if tags.at(2 * i).value == "ins" { st.ins } else { st.del }
    place(dx: x - st.bar-width / 2, dy: y0, rect(width: st.bar-width, height: calc.max(y1 - y0, 0pt), fill: c))
    i += 1
  }
}

#let redline-strike-math(c, it) = context {
  let s = measure(it)
  box({
    it
    place(top + left, dy: s.height / 2, line(length: s.width, stroke: 0.7pt + c))
  })
}

// Whole new blocks are coloured; new words inside a kept block are also
// filled and underlined, which stays visible in greyscale. A fill would
// patch every label of an inserted drawing, so blocks get none.
#let redline-ins-rules(body, block) = {
  let st = redline-style
  set text(fill: st.ins)
  if block { return body }
  // A box keeps a fill behind math but stops it from breaking across lines.
  show math.equation.where(block: false): it => context if measure(it).width < 0.2 * page.width {
    box(fill: st.ins-fill, outset: (y: 2pt), it)
  } else { it }
  highlight(fill: st.ins-fill, stroke: (bottom: 0.5pt + st.ins), body)
}

// Whether content holds something that would affect the rest of the
// document: a label, a footnote, a reference, a state or counter update.
#let redline-effect(c) = type(c) == content and (
  c.func() in (footnote, ref) or c.has("label") or repr(c.func()) in ("state-update", "counter-update")
    or c.fields().values().any(v => redline-effect(v) or type(v) == array and v.any(redline-effect))
)

// Deleted content without effects on the rest of the document: labels and
// updates removed, footnotes inline, references to a target or citation
// the new document lacks as text. Show rules cannot do this, because an
// element is registered before they run, so the content is rebuilt; styles
// on the way to such an element are lost.
#let redline-neutral(c) = {
  if not redline-effect(c) { return c }
  let f = c.func()
  if repr(f) in ("state-update", "counter-update") { return [] }
  if f == footnote { return [ (#redline-neutral(c.body))] }
  if f == ref {
    // Citations cannot be queried before Typst 0.14.
    return context if query(c.target).len() > 0 or sys.version >= version(0, 14, 0) and query(cite).any(x => x.key == c.target) { c } else {
      "@" + str(c.target)
    }
  }
  let fs = c.fields()
  let _ = fs.remove("label", default: none)
  if f == [].func() { return fs.children.map(redline-neutral).join() }
  if repr(f) == "styled" { return redline-neutral(fs.child) }
  let keys = ("dest", "alignment", "angle", "count").filter(k => k in fs)
  let positional = keys.map(k => fs.at(k))
  for k in keys { let _ = fs.remove(k) }
  let body = if "body" in fs { (redline-neutral(fs.remove("body")),) } else { () }
  let children = fs.remove("children", default: ()).map(redline-neutral)
  f(..fs, ..positional, ..body, ..children)
}

// Deleted content must not step equation, figure or heading counters. Its
// display equations and figures are outlined, which shows in greyscale.
#let redline-del-rules(body) = {
  let st = redline-style
  let frame(it) = block(stroke: (paint: st.del, thickness: 0.6pt, dash: "dashed"), inset: 3pt, width: 100%, it)
  set math.equation(numbering: none)
  set figure(numbering: none)
  set heading(numbering: none, outlined: false, bookmarked: false)
  show math.equation.where(block: false): it => redline-strike-math(st.del, it)
  show math.equation.where(block: true): frame
  show figure: frame
  text(fill: st.del, strike(stroke: 0.7pt + st.del, redline-neutral(body)))
}

// `block`: a whole paragraph, item, heading or code expression; `heading`:
// the level of a deleted heading.
#let redline-ins(body, block: false) = {
  redline-tags.ins
  redline-ins-rules(body, block)
  redline-tags.end
}

#let redline-del(body, block: false, heading: none) = {
  let body = if heading != none {
    text(weight: "bold", size: if heading == 1 { 1.4em } else { 1.2em }, body)
  } else { body }
  // A deleted block is written as a continuation of the block before it.
  if block or heading != none { parbreak() }
  redline-tags.del
  redline-del-rules(body)
  redline-tags.end
  // Keeps a deletion apart from an insertion right after it.
  if not block and heading == none { h(0.2em) }
}

// info: (old, new: version labels, ins, del: numbers of changes, entries:
// array of (file, level, target: label or none, title, count, deleted, unmarked))
#let redline-summary(info) = {
  let st = redline-style
  // One block, so that slide packages keep the summary together.
  block({
    block(text(size: 1.4em, weight: "bold")[Changes from #info.old to #info.new])
    if info.entries.len() == 0 { [No changes.] } else {
      [#redline-ins-rules([Inserted], false) and #redline-del-rules[deleted] text; a bar in the margin marks each changed line. Numbers count changed passages.]
    }
    let min-level = calc.min(..info.entries.map(e => e.level), 9)
    for e in info.entries {
      let note = if e.unmarked [, also code changes that are not marked]
      let line = if e.deleted {
        text(fill: st.del)[#strike(e.title) (deleted section)]
      } else if e.target == none {
        [#e.title (#e.count#note)]
      } else {
        let num = context {
          let el = query(e.target)
          if el.len() > 0 and el.first().func() == heading and el.first().numbering != none {
            numbering(el.first().numbering, ..counter(heading).at(e.target)) + h(0.5em)
          }
        }
        let pg = context counter(page).at(e.target).first()
        [#link(e.target, underline[#num#e.title]) (#e.count#note)#box(width: 1fr, repeat[#h(0.4em).])#h(0.4em)#link(e.target, pg)]
      }
      block(above: 0.5em, below: 0.5em, pad(left: (e.level - min-level) * 1.5em, line))
    }
  })
  pagebreak()
  counter(page).update(1)
}
