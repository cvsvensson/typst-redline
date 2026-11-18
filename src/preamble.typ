// typst-redline: how changes look. To restyle, pass a file of your own with
// --preamble. The marked files import four names from it:
// - redline-ins(body, block: false): inserted content; `block` when it is a
//   whole paragraph, item, heading or code expression.
// - redline-del(body, block: false, heading: none, gap: false): deleted
//   content; `heading` is the level of a deleted heading, `gap` says that an
//   insertion follows. A deleted block is written as a continuation of the
//   block before it, so a block deletion must start its own paragraph.
// - redline-bars(body): the rest of the document, which draws change bars on
//   its pages (`body => body` for nothing).
// - redline-summary(info): the summary; `info` is (old, new: version labels,
//   ins, del: numbers of changes, entries: array of (file, level, target:
//   label or none, title, count, deleted, new, code: array of changed code
//   that is not marked), bib: (added, removed, changed: arrays of citation
//   keys whose bibliography entry is new, gone or edited), and if the
//   versions are git revisions, history: (commits: array of (hash, date,
//   subject), oldest first, earlier: number of earlier commits left out,
//   uncommitted: whether the new version has uncommitted changes)).
// With `redline-ins(body, ..args) = body` and `redline-del(..args) = none`
// the document is the new version unchanged; marks must keep that so.
// For the side-by-side layout the binary compiles the marked files with
// `--input redline-side=new`, and the old version marked against the new
// one with `redline-side=old`, so that there an insertion is content only
// the old version has. The composite of both calls
// redline-side-by-side(info) (see there).

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
// records the width of the column, cell or other container it is in and the
// column gutter in effect; the word joiner keeps a line break from
// separating it from the mark.
#let redline-tag(kind, dy, width: none) = if kind != "end" or width != none {
  [#metadata((kind, dy, width, if width != none { columns.gutter }))<redline-tag>]
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
// frame. An image, a shape or content computed at layout (a drawing) is,
// when it is all of a block mark. Names, because some shapes exist only in
// some Typst versions.
#let redline-framed(c, sole) = (
  c.func() in (figure, block, align, grid, table, stack, pad, columns)
    or c.func() == math.equation and c.block
    or sole and repr(c.func()) in ("image", "rect", "square", "circle", "ellipse", "polygon", "line", "path", "curve", "context", "layout")
)

// A frame around a block-level element, outside it so that nothing moves;
// as wide as the element. It carries the mark's tags when the element
// starts or ends the mark, so that the bar spans the frame.
#let redline-frame(it, stroke, cross: false, start: none, end: none) = block(width: 100%, layout(size => {
  let st = redline-style
  let eq = it.func() == math.equation
  let natural = measure(if eq { math.equation(block: true, numbering: none, it.body) } else { it })
  let h = measure(block(width: size.width, it)).height
  // `measure` cannot resolve a relative width, so it is resolved here. An
  // element whose height depends on the width it gets (fractional columns,
  // wrapped text) fills the width.
  let rel = if it.func() == figure { it.body } else { it }
  let rel = if type(rel) == content { rel.at("width", default: auto) } else { auto }
  let w = if type(rel) == ratio { calc.min(rel * size.width, size.width) } else if type(rel) == relative {
    calc.min(rel.ratio * size.width + rel.length.to-absolute(), size.width)
  } else if natural.width == 0pt or natural.width > size.width or calc.abs((natural.height - h).to-absolute()) > 0.1pt { size.width } else { natural.width }
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
// of parts that are not framed, `frame-style` to each framed part that shows
// text. A drawing or image is drawn as written, with only `plain` applied:
// its frame (crossed, if deleted) is the mark, and a colour would reach its
// labels but not its lines. A figure's caption is styled all the same.
#let redline-mark(kind, body, block, style, stroke, frame-style: x => x, plain: x => x) = {
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
      let drawing = not redline-textual(if c.func() == figure { c.body } else { c })
      let f = redline-frame(c, stroke, cross: kind == "del" and drawing, start: start, end: if i == n - 1 { "end" })
      out.push(if not drawing { frame-style(f) } else if c.func() == figure {
        show figure.caption: frame-style
        plain(f)
      } else { plain(f) })
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
// Whether a deleted citation of `key` can be laid out here without changing
// the new document's bibliography or numbering: the new document cites the
// key, and cites no other key for the first time between here and its first
// citation of the key.
#let redline-cite-safe(key) = {
  let new(cites) = cites.filter(x => not (x.has("label") and x.label == <redline-deleted-cite>))
  let all = new(query(cite))
  let first = all.position(x => x.key == key)
  if first == none { return false }
  let i = new(query(selector(cite).before(here()))).len()
  let seen = all.slice(0, i).map(x => x.key)
  all.slice(calc.min(i, first), first).all(x => x.key in seen)
}

#let redline-neutral(c) = {
  if not redline-effect(c) { return c }
  let f = c.func()
  if repr(f) in ("state-update", "counter-update") { return [] }
  if f == footnote { return [ (#redline-neutral(c.body))] }
  if f == ref {
    let sup = c.at("supplement", default: none)
    // Citations cannot be queried before Typst 0.14.
    return context if query(c.target).len() > 0 { c } else if sys.version >= version(0, 14, 0) and redline-cite-safe(c.target) {
      [#cite(c.target, supplement: if sup == auto { none } else { sup })<redline-deleted-cite>]
    } else {
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
#let redline-del-counters(body) = {
  set math.equation(numbering: none)
  set figure(numbering: none)
  set heading(numbering: none, outlined: false, bookmarked: false)
  body
}

#let redline-del-look(body) = {
  let st = redline-style
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

#let redline-del-rules(body) = redline-del-counters(redline-del-look(body))

// The version laid out for the side-by-side layout, or none.
#let redline-side = sys.inputs.at("redline-side", default: none)

// On each side of the side-by-side layout, every change records its kind
// as a change from the old version to the new one, and an identity, so that
// the binary can pair the pages of the two sides.
#let redline-hash(seed, body) = {
  let h = seed
  for b in array(bytes(repr(body))) { h = calc.rem(h * 31 + b, 1000000007) }
  h
}

#let redline-side-mark(kind, body) = {
  let kind = if redline-side == "old" { if kind == "ins" { "del" } else { "ins" } } else { kind }
  [#metadata((kind, redline-hash(if kind == "ins" { 1 } else { 2 }, body)))<redline-change>]
}

#let redline-ins(body, block: false) = {
  let st = redline-style
  if redline-side != none { redline-side-mark("ins", body) }
  if redline-side == "old" {
    redline-mark("del", body, block, redline-del-look, st.del-frame, frame-style: redline-del-look)
  } else {
    redline-mark("ins", body, block, b => redline-ins-rules(b, block), st.ins-frame, frame-style: text.with(fill: st.ins))
  }
}

// On a side of the side-by-side layout, a deletion is content the version
// does not have: the page next to it shows it.
#let redline-del(body, block: false, heading: none, gap: false) = {
  let st = redline-style
  if redline-side != none { return redline-side-mark("del", body) }
  let block = block or heading != none
  let body = redline-neutral(body)
  let body = if heading != none {
    text(weight: "bold", size: if heading == 1 { 1.4em } else { 1.2em }, body)
  } else { body }
  if block { parbreak() }
  redline-mark("del", body, block, redline-del-rules, st.del-frame, frame-style: redline-del-rules, plain: redline-del-counters)
  if block { parbreak() } else if gap { h(0.25em) }
}

// The bars of one page, drawn from the page foreground or background
// (`from`): one query per page instead of introspection per mark.
#let redline-page-bars(from) = context {
  let st = redline-style
  let pg = here().page()
  [#metadata(from)<redline-page>]
  if pg == 1 {
    // For the summary and the binary: the pages with changes but without
    // bars. For the side-by-side layout also the number of pages, each
    // change's kind (0 deleted, 1 inserted), identity and page, and the same
    // for each heading (kind 2), whose identity is its text, all as one flat
    // array of integers.
    let drawn = query(<redline-page>).map(m => m.location().page())
    let unbarred = query(<redline-tag>).map(t => t.location().page()).dedup().filter(p => p not in drawn)
    [#metadata(unbarred)<redline-unbarred>]
    if redline-side != none {
      let changes = query(<redline-change>).map(m => (int(m.value.at(0) == "ins"), m.value.at(1), m.location().page()))
      let headings = query(heading).map(h => (2, redline-hash(3, h.body), h.location().page()))
      [#metadata((drawn.len() + unbarred.len(), unbarred.len(), ..unbarred, ..changes.flatten(), ..headings.flatten()))<redline-layout>]
    }
  }
  let tags = query(<redline-tag>)
  let n = calc.quo(tags.len(), 2)
  if n == 0 { return }
  let (w, h) = (page.width, page.height)
  let m = page.margin
  let even = calc.even(pg)
  let absolute(v, full) = if type(v) == length { v.to-absolute() } else if type(v) == ratio { v * full } else {
    v.length.to-absolute() + v.ratio * full
  }
  let side(keys, full) = {
    let v = if type(m) == dictionary { keys.map(k => m.at(k, default: none)).find(v => v != none) } else { m }
    if v == none or v == auto { 2.5 / 21 * calc.min(w, if h == auto { w } else { h }) } else {
      absolute(v, if full == auto { 0pt } else { full })
    }
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
      // A container of width `cw` that is one of `cols` equal columns with
      // the gutter in effect, as page columns and `columns` are, has its
      // bar just left of it: in the margin or in the middle of the gutter,
      // with the lanes narrowed to fit. Any other container's left edge is
      // unknown and text can run up to it, so its bar goes in a margin: the
      // right one if the container cannot reach the left edge of the text
      // area but can reach its right edge, such as a right-hand cell.
      // Deletions are one lane further out.
      let cw = b.value.at(2, default: none)
      let cw = if cw == none or cw <= 0pt { body } else { cw }
      let g = b.value.at(3, default: none)
      let g = if g == none { 0pt } else { absolute(g, body) }
      let q = (body + g) / (cw + g)
      let cols = if calc.abs(q - calc.round(q)) < 0.001 { int(calc.round(q)) } else { 1 }
      let k = calc.clamp(calc.floor(((if s.page == pg { s.x } else { e.x }) - left) / (cw + g)), 0, cols - 1)
      let at = calc.min(s.x, e.x) - left
      let out = if ins { 0pt } else { st.lane }
      let x = if cols > 1 and k > 0 {
        let lane = calc.max(0pt, calc.min(st.lane, (g - st.bar-width) / 2))
        left + k * (cw + g) - calc.min(st.bar-gap, g / 2) + lane / 2 - (if ins { 0pt } else { lane })
      } else if cols == 1 and at > cw and at + cw > body - 1pt {
        calc.min(left + body + calc.min(st.bar-gap, right / 2) + out, w - st.bar-width / 2)
      } else {
        calc.max(left - calc.min(st.bar-gap, left / 2) - out, st.bar-width / 2)
      }
      bars.push((x, y0 + 1pt, y1 - 1pt, ins))
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

// The rest of the document, with the bars drawn over the page foreground in
// effect here. A page whose foreground the document replaces later gets them
// under its background instead; the page background is drawn below the
// content, but the bars are in the margin.
#let redline-bars(body) = context {
  let (fg, bg) = (page.foreground, page.background)
  let fallback = context if query(<redline-page>).all(m => m.value != "fg" or m.location().page() != here().page()) {
    redline-page-bars("bg")
  }
  set page(foreground: { fg; redline-page-bars("fg") }, background: { bg; fallback })
  body
}

// The bibliography entries that changed, as a sentence, or none.
#let redline-bib(bib) = {
  let bib = (([new], bib.added), ([no longer cited], bib.removed), ([changed], bib.changed)).filter(p => p.at(1).len() > 0)
  if bib.len() > 0 [Bibliography entries, not marked in the bibliography: #bib.map(p => [#p.at(0): #p.at(1).map(raw).join(", ")]).join("; ").]
}

// The commits from the old version to the new one; none without a
// history.
#let redline-commits(hist, old, new) = if hist != none {
  let rows = hist.commits.map(c => (raw(c.hash), c.date, c.subject))
  if hist.uncommitted { rows.push(([], [], emph[Uncommitted changes])) }
  block(below: 1.5em, {
    block(below: 0.8em, strong[Commits from #old to #new])
    if hist.earlier > 0 { block(emph[#hist.earlier earlier commits, not listed]) }
    grid(columns: (auto, auto, 1fr), column-gutter: 0.6em, row-gutter: 0.6em, ..rows.flatten())
  })
}

// A line of changed code that is not marked, shortened.
#let redline-code(text) = {
  let t = text.clusters()
  raw(if t.len() > 80 { t.slice(0, 80).join() + "…" } else { text })
}

#let redline-summary(info) = if redline-side == none {
  let st = redline-style
  // One block, so that slide packages keep the summary together.
  block({
    block(text(size: 1.4em, weight: "bold")[Changes from #info.old to #info.new])
    let bib = redline-bib(info.at("bib", default: (added: (), removed: (), changed: ())))
    if info.entries.len() == 0 and bib == none { [No changes.] } else {
      let frame(stroke, body) = box(stroke: stroke, inset: (x: 2pt), outset: (y: 2pt), body)
      block(below: 1.5em)[#redline-ins-rules([Inserted], false) and #redline-del-rules[deleted] text; #text(fill: st.ins)[new paragraphs] in blue. New equations, figures and drawings are #frame(st.ins-frame)[framed], deleted ones #frame(st.del-frame)[framed dashed] and crossed where they cannot be struck through. Bars in the margin: solid for inserted lines, dashed and further out for deleted ones. Numbers count changed passages.]
    }
    redline-commits(info.at("history", default: none), info.old, info.new)
    if bib != none { block(below: 1.5em, bib) }
    context {
      let unbarred = query(<redline-unbarred>).map(m => m.value).flatten()
      if unbarred.len() > 0 {
        block(text(fill: st.del)[Changes on these pages of this PDF have no bars in the margin, because the document sets its own page foreground and background there: #unbarred.map(str).join(", ").])
      }
    }
    let min-level = calc.min(..info.entries.map(e => e.level), 9)
    for e in info.entries {
      let note = if e.code.len() > 0 [; changed code not marked]
      let line = if e.deleted {
        text(fill: st.del)[#strike(e.title) (deleted section#note)]
      } else if e.target == none {
        [#e.title (#e.count#note)]
      } else {
        let num = context {
          let el = query(e.target)
          if el.len() > 0 and el.first().func() == heading and el.first().numbering != none {
            numbering(el.first().numbering, ..counter(heading).at(e.target)) + h(0.5em)
          }
        }
        // The number printed on the page, or the page of this PDF.
        let pg = context {
          let loc = query(e.target).first().location()
          if loc.page-numbering() == none { loc.page() } else { counter(page).at(loc).first() }
        }
        let new = if e.new [new section, ]
        [#link(e.target, underline[#num#e.title]) (#new#e.count#note)#box(width: 1fr, repeat[#h(0.4em).])#h(0.4em)#link(e.target, pg)]
      }
      block(above: 0.5em, below: 0.5em, pad(left: (e.level - min-level) * 1.5em, {
        line
        if e.code.len() > 0 { pad(left: 1.5em, text(size: 0.85em, e.code.map(redline-code).join(linebreak()))) }
      }))
    }
  })
  pagebreak()
  counter(page).update(1)
}

// The side-by-side layout. `info` is (old, new: (label, pdf: the version
// compiled with `redline-side`, pages, unbarred: pages with changes but no
// bars), pairs: array of (old, new: pages
// shown next to each other, steps-old, steps-new: pages of earlier steps of
// the same slide, not shown), unpaired: array of (side, ins, page): changes
// found in one version only, unmarked: array of (file, text): changed code
// that is not marked, bib: as for `redline-summary`).
#let redline-side-by-side(info) = context {
  let st = redline-style
  let (o, n) = (info.old, info.new)
  let (wo, wn) = (measure(image(o.pdf, page: 1)).width, measure(image(n.pdf, page: 1)).width)
  let width = wo + wn
  // Pages are shown at their own size, so text is sized to them: about the
  // size of body text on a page that wide.
  let size = calc.max(calc.max(wo, wn) / 50, 10pt)
  let gap = 2 * size
  let pages(ps) = {
    let runs = ()
    for p in ps {
      if runs.len() > 0 and runs.last().last() + 1 == p { runs.last().push(p) } else { runs.push((p,)) }
    }
    runs.map(r => str(r.first()) + if r.len() > 1 { "–" + str(r.last()) } else { "" }).join(", ", default: "–")
  }
  let shown(ps, steps) = if ps.len() == 0 [–] else [#pages(ps)#if steps.len() > 0 [ (last step of #pages(steps + ps))]]
  let name(side) = text(weight: "bold", fill: if side == o { st.del } else { st.ins }, side.label)
  let left-out(side, ps) = side.pages - ps.map(p => p.len()).sum(default: 0)
  set page(width: auto, height: auto, margin: gap)
  set text(size: size)
  block(width: width + gap, align(center, block(width: calc.min(width + gap, 36 * size), align(left, {
    block(text(size: 1.4em, weight: "bold")[Changes from #o.label to #n.label])
    let (lo, ln) = (left-out(o, info.pairs.map(p => p.old + p.steps-old)), left-out(n, info.pairs.map(p => p.new + p.steps-new)))
    if info.pairs.len() == 0 [No changes.] else [
      Pages of #name(o) on the left, with #redline-del-rules[deleted] text; pages of #name(n) on the right, with #redline-ins-rules([inserted], false) text. Bars in the margin mark the changed lines.
      #if lo + ln > 0 [Pages without changes are left out: #ln of the #n.pages pages of #n.label and #lo of the #o.pages of #o.label.]
    ]
    let at = 2
    let rows = ()
    for p in info.pairs {
      let k = calc.max(p.old.len(), p.new.len())
      rows += (shown(p.old, p.steps-old), shown(p.new, p.steps-new), link((page: at, x: 0pt, y: 0pt), underline(pages(range(at, at + k)))))
      at += k
    }
    if rows.len() > 0 {
      table(
        columns: 3,
        stroke: none,
        inset: (x: 0.5em, y: 0.3em),
        align: (left, left, right),
        table.header(..([Pages of #name(o)], [Pages of #name(n)], [Shown on page]).map(strong)),
        table.hline(stroke: 0.5pt),
        ..rows,
      )
    }
    if info.unpaired.len() > 0 {
      [Changes found in one version only; their pages are shown without a counterpart:]
      list(..info.unpaired.map(u => [#if u.ins [An insertion] else [A deletion] on page #u.page of #info.at(u.side).label]))
    }
    if info.unmarked.len() > 0 {
      [Changed code that is not marked:]
      list(..info.unmarked.map(u => [#redline-code(u.text) in #raw(u.file)]))
    }
    let bib = redline-bib(info.bib)
    if bib != none { block(bib) }
    redline-commits(info.at("history", default: none), o.label, n.label)
    for side in (o, n) {
      if side.unbarred.len() > 0 { block[Changes on these pages of #name(side) have no bars in the margin, because the document sets its own page foreground and background there: #pages(side.unbarred).] }
    }
  }))))
  let head(side, pg, s, ps) = if pg != none [#name(side), page #pg#if s.len() > 0 [, the last step of #pages(s + (pg,))]] else if ps.len() == 0 [Not in #name(side)]
  // A side without a page keeps its width, so that the other stays in place.
  let shot(side, w, pg) = if pg == none { box(width: w) } else { box(stroke: 0.5pt + luma(150), image(side.pdf, page: pg)) }
  for p in info.pairs {
    for r in range(calc.max(p.old.len(), p.new.len())) {
      pagebreak()
      let (po, pn) = (p.old.at(r, default: none), p.new.at(r, default: none))
      grid(columns: 2, column-gutter: gap, row-gutter: 0.5em, head(o, po, p.steps-old, p.old), head(n, pn, p.steps-new, p.new), shot(o, wo, po), shot(n, wn, pn))
    }
  }
}
