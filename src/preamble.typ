// typst-redline: how changes look. To restyle, pass a file of your own with
// --preamble. The marked files import four names from it:
// - redline-ins(body, block: false): inserted content; `block` when it is a
//   whole paragraph, item, heading or code expression.
// - redline-del(body, block: false, heading: none, gap: false): deleted
//   content; `heading` is the level of a deleted heading, `gap` says that an
//   insertion follows. A deleted block is written as a continuation of the
//   block before it, so a block deletion must start its own paragraph.
// - redline-bars: the page foreground (`none` for nothing).
// - redline-summary(info): the summary; `info` is (old, new: version labels,
//   ins, del: numbers of changes, entries: array of (file, level, target:
//   label or none, title, count, deleted, new, unmarked)).
// With `redline-ins(body, ..args) = body` and `redline-del(..args) = none`
// the document is the new version unchanged; marks must keep that so.

#let redline-style = {
  let (ins, del) = (rgb("#1565c0"), rgb("#c62828"))
  (
    ins: ins,
    ins-fill: rgb("#d8eaff"),
    del: del,
    strike: 0.7pt + del,
    // Frames of block-level elements, this far outside them.
    ins-frame: 0.6pt + ins,
    del-frame: (paint: del, thickness: 0.6pt, dash: "dashed"),
    outset: 3pt,
    // Change bars sit this far outside the text area, at most half the
    // margin; deletion bars one lane further out, so that the two never
    // touch.
    bar-gap: 0.5cm,
    bar-width: 2pt,
    lane: 3.5pt,
  )
}

// Each mark is bracketed by two of these; `redline-bars` draws a bar in the
// margin from the start of one to its end, `dy` beyond the tags. The end tag
// records the width of the column, cell or other container it is in; the
// word joiner keeps a line break from separating it from the mark.
#let redline-tag(kind, dy, width: none) = if kind != "end" or width != none {
  [#metadata((kind, dy, width))<redline-tag>]
} else [\u{2060}#box(layout(size => redline-tag(kind, dy, width: size.width)))]

// Spaces at the edges of a mark go outside it: a struck space looks like a
// hyphen, and a line break at it would put the bar on the line before.
#let redline-parts(body) = {
  let ch = if type(body) == content and body.func() == [].func() { body.children } else { (body,) }
  let sp(c) = type(c) == content and c.func() in ([ ].func(), parbreak)
  let lead = ()
  while ch.len() > 0 and sp(ch.first()) { lead.push(ch.remove(0)) }
  let trail = ()
  while ch.len() > 0 and sp(ch.last()) { trail.insert(0, ch.pop()) }
  (lead.join(), ch, trail.join())
}

// Whether content shows text that a colour or strike reaches; content
// computed at layout (a drawing) counts as not.
#let redline-textual(c) = type(c) == content and (
  c.func() in (text, math.equation, raw)
    or c.fields().values().any(v => redline-textual(v) or type(v) == array and v.any(redline-textual))
)

// Whether a part of a mark is laid out as a block of its own and gets a
// frame. An image or content computed at layout (a drawing) is, when it is
// all of a block mark.
#let redline-framed(c, sole) = (
  c.func() in (figure, block, align, grid, table, stack, pad, columns)
    or c.func() == math.equation and c.block
    or sole and (c.func() == image or repr(c.func()) in ("context", "layout"))
)

// A frame around a block-level element, outside it so that nothing moves;
// as wide as the element. It carries the mark's tags when the element
// starts or ends the mark, so that the bar spans the frame.
#let redline-frame(it, stroke, cross: false, start: none, end: none) = block(width: 100%, layout(size => {
  let st = redline-style
  let eq = it.func() == math.equation
  let natural = measure(if eq { math.equation(block: true, numbering: none, it.body) } else { it })
  let h = measure(block(width: size.width, it)).height
  // An element whose height depends on the width it gets (fractional
  // columns, wrapped text) fills the width.
  let w = if natural.width == 0pt or natural.width > size.width or calc.abs((natural.height - h).to-absolute()) > 0.1pt { size.width } else { natural.width }
  let a = if it.func() == align { it.alignment } else if eq or it.func() == figure { center } else { align.alignment }
  let a = if a.axis() == "horizontal" { a } else if a.axis() == none { a.x } else { start }
  it
  place(top + a, box(width: w, height: h, {
    place(rect(width: 100%, height: 100%, stroke: stroke, outset: st.outset))
    if cross {
      let s = 0.6pt + stroke.paint.transparentize(40%)
      place(line(start: (0pt, 0pt), end: (w, h), stroke: s))
      place(line(start: (0pt, h), end: (w, 0pt), stroke: s))
    }
    place(top + left, if start != none { redline-tag(start, -st.outset) })
    place(bottom + left, if end != none { redline-tag("end", st.outset, width: size.width) })
  }))
}))

// The parts of a mark with its tags and frames: `style` applied to each run
// of parts that are not framed, `frame-style` to each framed part.
#let redline-mark(kind, body, block, style, stroke, frame-style: x => x, cross: c => false) = {
  let (lead, ch, trail) = redline-parts(body)
  let n = ch.len()
  let framed = ch.map(c => redline-framed(c, block and n == 1))
  let out = ()
  let run = ()
  for (i, c) in ch.enumerate() {
    if framed.at(i) {
      if run.len() > 0 { out.push(style(run.join())) }
      run = ()
      let start = if i == 0 { kind }
      out.push(frame-style(redline-frame(c, stroke, cross: cross(c), start: start, end: if i == n - 1 { "end" })))
    } else if i == 0 and not block and c.func() == text and c.has("text") and c.text.clusters().len() > 1 {
      // After the first character, so that the bar of a mark starting a
      // line is not drawn on the line before.
      let cl = c.text.clusters()
      run += (text(cl.first()), redline-tag(kind, -0.8em), text(cl.slice(1).join()))
    } else {
      if i == 0 { run.push(redline-tag(kind, -0.8em)) }
      run.push(c)
    }
  }
  if run.len() > 0 { out.push(style(run.join())) }
  lead
  if n == 0 { redline-tag(kind, -0.8em) }
  out.join()
  if n == 0 or not framed.last() { redline-tag("end", 0.3em) }
  trail
}

// Whole new blocks are coloured; new words inside a kept block are also
// filled and underlined, which stays visible in greyscale.
#let redline-ins-rules(body, block) = {
  let st = redline-style
  if block { return text(fill: st.ins, body) }
  set text(fill: st.ins)
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
      emph(str(c.target))
    }
  }
  let fs = c.fields()
  let _ = fs.remove("label", default: none)
  if f == [].func() { return fs.children.map(redline-neutral).join() }
  if repr(f) == "styled" { return redline-neutral(fs.child) }
  let keys = ("dest", "alignment", "angle", "count", "value").filter(k => k in fs)
  let positional = keys.map(k => fs.at(k))
  for k in keys { let _ = fs.remove(k) }
  let body = if "body" in fs { (redline-neutral(fs.remove("body")),) } else { () }
  let children = fs.remove("children", default: ()).map(redline-neutral)
  f(..fs, ..positional, ..body, ..children)
}

// Deleted content must not step equation, figure or heading counters.
#let redline-del-rules(body) = {
  let st = redline-style
  set math.equation(numbering: none)
  set figure(numbering: none)
  set heading(numbering: none, outlined: false, bookmarked: false)
  show math.equation.where(block: false): it => context {
    let s = measure(it)
    box({
      it
      place(top + left, dy: s.height / 2, line(length: s.width, stroke: st.strike))
    })
  }
  // A document's own text colours must not make a deletion look inserted.
  show text: set text(fill: st.del)
  text(fill: st.del, strike(stroke: st.strike, body))
}

#let redline-ins(body, block: false) = {
  let st = redline-style
  redline-mark("ins", body, block, b => redline-ins-rules(b, block), st.ins-frame, frame-style: text.with(fill: st.ins))
}

#let redline-del(body, block: false, heading: none, gap: false) = {
  let st = redline-style
  let block = block or heading != none
  let body = redline-neutral(body)
  let body = if heading != none {
    text(weight: "bold", size: if heading == 1 { 1.4em } else { 1.2em }, body)
  } else { body }
  if block { parbreak() }
  redline-mark("del", body, block, redline-del-rules, st.del-frame, frame-style: redline-del-rules, cross: c => not redline-textual(
    if c.func() == figure { c.body } else { c },
  ))
  if block { parbreak() } else if gap { h(0.25em) }
}

// The page foreground: one query per page instead of introspection per mark.
#let redline-bars = context {
  let st = redline-style
  let tags = query(<redline-tag>)
  let n = calc.quo(tags.len(), 2)
  if n == 0 { return }
  let pg = here().page()
  let (w, h) = (page.width, page.height)
  let m = page.margin
  let even = calc.even(pg)
  let side(keys, full) = {
    let v = if type(m) == dictionary { keys.map(k => m.at(k, default: none)).find(v => v != none) } else { m }
    let full = if full == auto { 0pt } else { full }
    if v == none or v == auto { 2.5 / 21 * calc.min(w, if h == auto { w } else { h }) } else if type(v) == length {
      v.to-absolute()
    } else if type(v) == ratio { v * full } else { v.length.to-absolute() + v.ratio * full }
  }
  let left = side(if even { ("left", "outside") } else { ("left", "inside") }, w)
  let right = side(if even { ("right", "inside") } else { ("right", "outside") }, w)
  let top = side(("top",), h)
  let bottom = if h == auto { 1e4pt } else { h - side(("bottom",), h) }
  let body = w - left - right
  // Tags come in document order, so the first pair ending on this page is
  // found by bisection.
  let (lo, hi) = (0, n)
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    if tags.at(2 * mid + 1).location().page() < pg { lo = mid + 1 } else { hi = mid }
  }
  let bars = ()
  let i = lo
  while i < n {
    let (a, b) = (tags.at(2 * i), tags.at(2 * i + 1))
    let (s, e) = (a.location().position(), b.location().position())
    if s.page > pg { break }
    // A mark ending above the text area is in a header or title bar.
    if e.page > pg or e.y > top {
      let y0 = if s.page == pg { calc.max(s.y + a.value.at(1).to-absolute(), top - 1em.to-absolute()) } else { top }
      let y1 = if e.page == pg { e.y + b.value.at(1).to-absolute() } else { bottom }
      let ins = a.value.at(0) == "ins"
      // The mark is in one of `cols` containers of width `cw` side by side
      // across the text area, as many as fit. Its bar goes just left of its
      // container: in the margin, or in the gutter, with the lanes narrowed
      // to fit it. Deletions are one lane further out.
      let cw = b.value.at(2, default: none)
      let cw = if cw == none or cw <= 0pt or cw > body { body } else { cw }
      let cols = calc.max(calc.floor(body / cw + 0.01), 1)
      let gap = if cols > 1 { (body - cols * cw) / (cols - 1) } else { 0pt }
      let at = s.x - left
      let k = calc.clamp(calc.max(calc.floor(at / (cw + gap)), calc.ceil((at - cw) / (cw + gap))), 0, cols - 1)
      let x = if k == 0 {
        let x = left - calc.min(st.bar-gap, left / 2)
        if ins { x } else { x - st.lane }
      } else {
        let lane = calc.max(0pt, calc.min(st.lane, (gap - st.bar-width) / 2))
        let x = left + k * (cw + gap) - calc.min(st.bar-gap, gap / 2) + lane / 2
        if ins { x } else { x - lane }
      }
      bars.push((calc.max(x, st.bar-width / 2), y0 + 1pt, y1 - 1pt, ins))
    }
    i += 1
  }
  // Overlapping bars of one kind in one place are drawn as one, so that
  // dashes do not overlap.
  let merged = ()
  for bar in bars.sorted(key: b => (b.at(0).pt(), b.at(1).pt())) {
    let last = if merged.len() > 0 { merged.last() }
    if last != none and last.at(0) == bar.at(0) and last.at(3) == bar.at(3) and bar.at(1) <= last.at(2) + 2pt {
      merged.last().at(2) = calc.max(last.at(2), bar.at(2))
    } else { merged.push(bar) }
  }
  for (x, y0, y1, ins) in merged {
    let stroke = (paint: if ins { st.ins } else { st.del }, thickness: st.bar-width, dash: if ins { none } else { (3pt, 2pt) })
    place(dx: x, dy: y0, line(angle: 90deg, length: calc.max(y1 - y0, 0pt), stroke: stroke))
  }
}

#let redline-summary(info) = {
  let st = redline-style
  // One block, so that slide packages keep the summary together.
  block({
    block(text(size: 1.4em, weight: "bold")[Changes from #info.old to #info.new])
    if info.entries.len() == 0 { [No changes.] } else {
      let frame(stroke, body) = box(stroke: stroke, inset: (x: 2pt), outset: (y: 2pt), body)
      [#redline-ins-rules([Inserted], false) and #redline-del-rules[deleted] text; #text(fill: st.ins)[new paragraphs] in blue. New equations, figures and drawings are #frame(st.ins-frame)[framed], deleted ones #frame(st.del-frame)[framed dashed] and crossed where they cannot be struck through. Bars in the margin: solid for inserted lines, dashed and further out for deleted ones. Numbers count changed passages.]
    }
    let min-level = calc.min(..info.entries.map(e => e.level), 9)
    for e in info.entries {
      let note = if e.unmarked [; changed code not marked]
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
        let new = if e.new [new section, ]
        [#link(e.target, underline[#num#e.title]) (#new#e.count#note)#box(width: 1fr, repeat[#h(0.4em).])#h(0.4em)#link(e.target, pg)]
      }
      block(above: 0.5em, below: 0.5em, pad(left: (e.level - min-level) * 1.5em, line))
    }
  })
  pagebreak()
  counter(page).update(1)
}
