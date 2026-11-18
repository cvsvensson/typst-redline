// typst-redline: how changes look. To restyle, pass a file of your own with
// --preamble. The marked files import four names from it:
// - redline-ins(body, block: false): inserted content; `block` when it is a
//   whole paragraph, item, heading or code expression.
// - redline-del(body, block: false, heading: none, gap: false): deleted
//   content; `heading` is the level of a deleted heading, `gap` says that an
//   insertion follows. A deleted block is written as a continuation of the
//   block before it, so a block deletion must start its own paragraph.
// - Moved text: `redline-del(move: m)[first words]` stands at its old place,
//   and `redline-ins(move: m)[]` at the start and, with `m.end`, at the end
//   of its new place, which holds it unmarked or with its edits marked.
//   `m` is (id: the move in this document, key: the same for the move back,
//   heading: the old place starts with a heading, down: the new place comes
//   later in the document, more: the passage goes on
//   after the first words, paragraphs, items: how many of each it holds
//   besides a heading, block: the marker stands next to a block-level
//   element or on a line of its own).
// - redline-bars(body): the rest of the document, which draws change bars on
//   its pages (`body => body` for nothing).
// - redline-summary(info): the summary, after the document; `info` is (old, new: version labels,
//   ins, del: numbers of changes, moved: number of moves, entries: array of
//   (file, level, target: label or none, title, count: changed passages,
//   moves: moves into or out of the section other than its own, before:
//   the changes before the first heading at the top level, deleted,
//   new, moved, code: array of (old, new): changed code that is not
//   marked, either empty if inserted or deleted, marks: array of (id,
//   move: number of the move if it is one, old, new): its marks), bib: (added,
//   removed, changed: arrays of citation keys whose bibliography entry is
//   new, gone or edited, files: paths of other files that the document
//   reads and that differ, unchecked: whether files read through a computed
//   path were not checked), and if the versions are git revisions,
//   history: (commits: array of (hash, date, subject), oldest first,
//   earlier: number of earlier commits left out, uncommitted: whether the
//   new version has uncommitted changes, steps: whether marks are coloured
//   by step, and with --steps groups: array of (from, to: revision labels,
//   commits: number of commits of the history in the step, the uncommitted
//   changes belonging to the last))).
// With --each-commit or --steps, marks take `step: n`: the change is from
// the n-th group, or else the n-th commit of the history or the uncommitted
// changes after the last.
// redline-step-style(n) gives the style of its marks. Every mark other
// than a move's takes `id: n`, which the summary uses to find marks that
// were not laid out.
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
    // Behind deleted text on a dark background.
    del-fill: rgb("#fde4e4"),
    // As thick as the text is large, so that it does not cover small text.
    strike: 0.065em + del,
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
    // Brackets and lines of moved text.
    move: luma(110),
    move-stroke: 0.8pt,
    // With --each-commit, the colour of each commit: Okabe–Ito hues, darkened
    // where needed to read as text on white.
    steps: (rgb("#0072b2"), rgb("#d55e00"), rgb("#007a5a"), rgb("#a8508a"), rgb("#9a6800"), rgb("#2f8fce")),
  )
}

// The colours of marks in one colour.
#let redline-colours(c) = (
  ins: c,
  ins-fill: c.lighten(80%),
  del: c,
  del-fill: c.lighten(85%),
  strike: 0.065em + c,
  ins-frame: 0.6pt + c,
  del-frame: (paint: c, thickness: 0.6pt, dash: "dashed"),
)

// The style of the marks of a step, or the plain style for none.
#let redline-step-style(step) = if step == none { redline-style } else {
  let cs = redline-style.steps
  redline-style + redline-colours(cs.at(calc.rem(step - 1, cs.len())))
}

// The tool's own bookkeeping: a labelled update of the state "redline",
// which the document's queries do not find as they find metadata.
// `redline-value` reads its value back. The binary reads the key, which for
// `redline-ints` lists the integers.
#let redline-meta(value, label, key: "redline") = [#state(key, none).update(value)#label]
#let redline-value(m) = state(m.key).at(m.location())
#let redline-ints(ints, label) = redline-meta(ints, label, key: if ints.len() == 0 { "" } else { ints.map(str).join(",") })

// Each mark is bracketed by two of these; `redline-bars` draws a bar in the
// margin from the start of one to its end, `dy` beyond the tags. The end tag
// records the width of the column, cell or other container it is in, the
// column gutter in effect and the number of page columns; the start tag
// carries the mark's id, for the summary. The word joiner keeps a line
// break from separating it from the mark. `redline-bars` labels the tag
// `<redline-tag>` where it is visible.
#let redline-tag(kind, dy, width: none, step: none, id: none) = if kind != "end" or width != none {
  redline-meta((kind, dy, width, if width != none { columns.gutter }, step, if width != none { page.columns }, id), <redline-tag-at>)
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

// The fallbacks the binary applied after the full proof failed to compile,
// from `--input redline-fallback=...`: "states" keeps state updates in
// deleted content, "frames" frames no content computed at layout, "names"
// shows deleted code by name, "whole" marks changed code whole.
#let redline-fallback = sys.inputs.at("redline-fallback", default: "").split(",")

// What the fallbacks show differently, for the summary; none without any.
// The note on states that deleted content updates covers "states".
#let redline-fallback-note = {
  let says = (
    frames: [Changed content that the document computes during layout, such as many drawings, has no frame; its bar in the margin still marks it.],
    names: [Deleted code is shown by its name instead of as it looked.],
    whole: [Code with changes inside its content arguments is marked whole: the old call deleted, the new one inserted.],
  )
  let lines = redline-fallback.filter(f => f in says).map(f => says.at(f))
  if lines.len() > 0 [The document did not compile with every change marked in full, so this PDF marks some changes more coarsely. #lines.join[ ]]
}

// Whether a part of a mark is laid out as a block of its own and gets a
// frame. An image, a shape or content computed at layout (a drawing) is,
// when it is all of a block mark. Names, because some shapes exist only in
// some Typst versions. Content computed at layout can also set the page,
// which no frame can hold, so the "frames" fallback leaves it unframed.
#let redline-framed(c, sole) = (
  c.func() in (figure, block, align, grid, table, stack, pad, columns)
    or c.func() in (math.equation, raw) and c.block
    or sole and repr(c.func()) in ("image", "rect", "square", "circle", "ellipse", "polygon", "line", "path", "curve")
    or sole and repr(c.func()) in ("context", "layout") and "frames" not in redline-fallback
)

// The two lines that cross out a deleted drawing of size `w` by `h`.
#let redline-cross(stroke, w, h) = {
  let s = 0.6pt + stroke.paint.transparentize(40%)
  place(top + left, line(start: (0pt, 0pt), end: (w, h), stroke: s))
  place(top + left, line(start: (0pt, h), end: (w, 0pt), stroke: s))
}

// A frame around a block-level element, outside it so that nothing moves;
// as wide as the element. It carries the mark's tags when the element
// starts or ends the mark, so that the bar spans the frame.
#let redline-frame(it, stroke, cross: false, start: none, end: none, step: none, id: none) = block(width: 100%, layout(size => {
  let st = redline-style
  let eq = it.func() == math.equation
  // Twice the width tells content that fills its width as unbounded room
  // does, and a drawing that fills its width needs finite room. Typst 0.11
  // measures only in unbounded room.
  let room = if sys.version >= version(0, 12, 0) { (width: 2 * size.width) } else { (:) }
  let natural = measure(if eq { math.equation(block: true, numbering: none, it.body) } else { it }, ..room)
  let h = measure(block(width: size.width, it)).height
  // `measure` cannot resolve a relative width, so it is resolved here. An
  // element whose height depends on the width it gets (fractional columns,
  // wrapped text) fills the width.
  let rel = if it.func() == figure { it.body } else { it }
  let rel = if type(rel) == content { rel.at("width", default: auto) } else { auto }
  let w = if type(rel) == ratio { calc.min(rel * size.width, size.width) } else if type(rel) == relative {
    calc.min(rel.ratio * size.width + rel.length.to-absolute(), size.width)
  } else if natural.width == 0pt or natural.width > size.width or calc.abs((natural.height - h).to-absolute()) > 0.1pt { size.width } else { natural.width }
  // A figure's caption can be wider than a body of fixed width.
  let fixed = type(rel) != ratio and (type(rel) != relative or rel.ratio == 0%)
  let w = if it.func() == figure and fixed and natural.width <= size.width { calc.max(w, natural.width) } else { w }
  let a =if it.func() == align { it.alignment } else if eq or it.func() == figure { center } else { align.alignment }
  let a = if a.axis() == "horizontal" { a } else if a.axis() == none { a.x } else { start }
  it
  // Content of no height, such as an empty header, gets no frame.
  let empty = h == 0pt and repr(it.func()) != "line"
  place(top + a, box(width: w, height: h, {
    if not empty { place(rect(width: 100%, height: 100%, stroke: stroke, outset: st.outset)) }
    // The cross leaves a figure's caption, which is struck, alone.
    let ch = if it.func() == figure { measure(block(width: size.width, it.body)).height } else { h }
    if cross and not empty { redline-cross(stroke, w, ch) }
    place(top + left, if start != none { redline-tag(start, -st.outset, step: step, id: id) })
    place(bottom + left, if end != none { redline-tag("end", st.outset, width: size.width) })
  }))
}))

// Content with the end tag appended inside its set rules, which can set the
// page: after them, the tag would get a page of its own.
#let redline-end-in(c) = if repr(c.func()) == "styled" {
  (c.func())(redline-end-in(c.child), c.styles)
} else { c + redline-tag("end", 0.3em) }

// Whether content takes no room in the flow, such as placed content or a
// counter update.
#let redline-roomless(c) = type(c) == content and (
  c.func() in (place, metadata, [ ].func(), parbreak)
    or repr(c.func()) in ("counter-update", "state-update")
    or c.func() == [].func() and c.children.all(redline-roomless)
    or repr(c.func()) == "styled" and redline-roomless(c.child)
)

// Whether content shows colours of its own that a mark's colour or strike
// would make hard to read: a raw block, or an element with a fill.
#let redline-own-colours(c) = type(c) == content and (
  c.func() == raw and c.block
    or c.func() not in (text, highlight) and c.at("fill", default: none) != none
    or c.fields().values().any(v => redline-own-colours(v) or type(v) == array and v.any(redline-own-colours))
)

// The parts of a mark with its tags and frames: `style` applied to each run
// of parts that are not framed, `frame-style` to each framed part that shows
// text. A drawing, an image, or content with colours of its own is drawn as
// written, with only `plain` applied: its frame is the mark. A colour would
// reach the labels of a drawing but not its lines. A deleted drawing is also
// crossed. A box in the text that shows no text is a drawing too, framed at
// its own size. A figure's caption is styled all the same. The tags around
// content that takes no room are placed, so that they do not form a line.
#let redline-mark(kind, body, block, style, stroke, frame-style: x => x, plain: x => x, step: none, id: none) = {
  let (lead, ch, trail) = redline-parts(body)
  let n = ch.len()
  let framed = ch.map(c => redline-framed(c, block and n == 1))
  let roomless = block and n > 0 and ch.all(redline-roomless)
  let styled = n > 0 and not roomless and not framed.last() and repr(ch.last().func()) == "styled"
  // A tag between blocks is at the top of the next line; the bar goes in
  // the gap above it.
  let start-dy = if roomless { -1em } else { -0.8em }
  let out = ()
  let run = ()
  for (i, c) in ch.enumerate() {
    if framed.at(i) {
      if run.len() > 0 { out.push(style(run.join())) }
      run = ()
      let start = if i == 0 { kind }
      let b = if c.func() == figure { c.body } else { c }
      let drawing = not redline-textual(b)
      let own = drawing or redline-own-colours(b)
      let f = redline-frame(c, stroke, cross: kind == "del" and drawing, start: start, end: if i == n - 1 { "end" }, step: step, id: id)
      out.push(if not own { frame-style(f) } else if c.func() == figure {
        show figure.caption: frame-style
        plain(f)
      } else { plain(f) })
    } else {
      // A tag in a box, joined to the text after it, starts the line the
      // text starts; a tag alone stays on the line before a break.
      if i == 0 { run.push(if block { redline-tag(kind, start-dy, step: step, id: id) } else [#box(redline-tag(kind, -0.8em, step: step, id: id))\u{2060}]) }
      let c = if c.func() == box and c.at("body", default: none) != none and not redline-textual(c) {
        box(stroke: stroke, outset: redline-style.outset, {
          c
          if kind == "del" { context { let s = measure(c); redline-cross(stroke, s.width, s.height) } }
        })
      } else { c }
      run.push(if i == n - 1 and styled { redline-end-in(c) } else { c })
    }
  }
  if run.len() > 0 { out.push(style(run.join())) }
  lead
  if n == 0 { redline-tag(kind, -0.8em, step: step, id: id) }
  out.join()
  // Content computed at layout can set the page, and anything visible after
  // it would get a page of its own; such a tag has no width of its container.
  if n == 1 and block and not framed.last() and repr(ch.last().func()) in ("context", "layout") {
    context redline-tag("end", 0.3em, width: 0pt)
  } else if roomless {
    place(layout(size => redline-tag("end", -0.2em, width: size.width)))
  } else if n == 0 or not (framed.last() or styled) { redline-tag("end", 0.3em) }
  trail
}

// Whether the text colour in effect is light, as it is on a dark
// background, where the colours of marks are hard to read.
#let redline-light-text() = type(text.fill) == color and {
  let (r, g, b) = text.fill.rgb().components(alpha: false)
  0.2126 * r + 0.7152 * g + 0.0722 * b > 60%
}

// Whole new blocks are coloured, lighter on a dark background; new words
// inside a kept block are also filled and underlined, which stays visible
// in greyscale and on any background.
#let redline-ins-rules(body, block, st: redline-style) = {
  if block { return context text(fill: if redline-light-text() { st.ins.lighten(60%) } else { st.ins }, body) }
  set text(fill: st.ins)
  // A box keeps a fill behind math but stops it from breaking across lines.
  show math.equation.where(block: false): it => context if measure(it).width < 0.2 * page.width {
    box(fill: st.ins-fill, outset: (y: 2pt), it)
  } else { it }
  highlight(fill: st.ins-fill, stroke: (bottom: 0.5pt + st.ins), body)
}

// Whether state updates in deleted content are kept. A state cannot be set
// back after the deletion (see `redline-restore`), so they are dropped unless
// the document fails to compile without them.
#let redline-keep-states = "states" in redline-fallback

// Whether content holds something that would affect the rest of the
// document: a label, a footnote, a reference, metadata, a state update.
#let redline-effect(c) = type(c) == content and (
  c.func() in (footnote, ref, metadata) or c.has("label") or not redline-keep-states and repr(c.func()) == "state-update"
    or c.fields().values().any(v => redline-effect(v) or type(v) == array and v.any(redline-effect))
)

// Deleted content without effects on the rest of the document: labels,
// metadata and state updates removed, footnotes inline, references to a
// target or citation the new document lacks as text. Show rules cannot do this,
// because an element is registered before they run, so the content is
// rebuilt; styles on the way to such an element are lost. Counter updates
// stay, so that the content is numbered as in the old version;
// `redline-restore` undoes them.
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
  if f == metadata or repr(f) == "state-update" { return [] }
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

// Deleted figures and headings stay out of outlines and bookmarks.
#let redline-del-unlisted(body) = {
  set figure(outlined: false)
  set heading(outlined: false, bookmarked: false)
  body
}

#let redline-counter-update = counter("").update(0).func()
#let redline-state-update = state("").update(none).func()

// The counters that deleted content can change: those that the document
// updates by hand, and those of headings, equations and each kind of figure.
// Calls of a function are cached, so this is one query per layout pass for
// all deletions.
#let redline-counter-keys() = {
  let keys = query(redline-counter-update).map(u => u.key)
  keys += (selector(heading), selector(math.equation)) + query(figure).map(f => figure.where(kind: f.kind))
  keys.dedup().filter(k => k != page)
}

// Placed after deleted content that starts at the last element labelled
// `start`: sets every counter that the content changed back to its value
// before it, whoever keeps the counter. A state cannot be set back, because
// its value before can depend on the initial value that each reader passes,
// which nothing records; neither can the page counter, which steps with the
// pages. The summary lists those that the content updates, such as states
// that a show rule updates.
#let redline-restore(start) = context {
  let start = query(selector(start).before(here())).last().location()
  let inside(s) = query(selector(s).after(start).before(here()))
  // By the change inside the content rather than to the value before it:
  // that value depends on the updates of every earlier deletion, so that
  // setting it would take a layout pass per deletion to settle. The update
  // holds only the change, so that it is the same in every pass.
  for k in redline-counter-keys() {
    let (b, a) = (counter(k).at(start), counter(k).at(here()))
    let d = b.enumerate().map(((i, v)) => a.at(i, default: 0) - v)
    if d.any(x => x != 0) or a.len() != b.len() {
      counter(k).update((..now) => d.enumerate().map(((i, x)) => calc.max(0, now.pos().at(i, default: 0) - x)))
    }
  }
  // Counting the state updates before each end, the tool's own apart, is
  // cheaper than finding those inside every deletion.
  let count(c) = c.at(here()).first() - c.at(start).first()
  let own(u) = u.has("label") and str(u.label).starts-with("redline-")
  let changed = if count(counter(redline-state-update)) != count(counter(redline-state-update.where(key: "redline"))) {
    inside(redline-state-update).filter(u => not own(u)).map(u => u.key).dedup()
  } else { () }
  let lost = changed + if inside(redline-counter-update.where(key: page)).len() > 0 { (page,) } else { () }
  if lost.len() > 0 { redline-meta(lost, <redline-unrestored>) }
}

#let redline-del-look(body, st: redline-style) = {
  show math.equation.where(block: false): it => context {
    let s = measure(it)
    box({
      it
      place(top + left, dy: s.height / 2, line(length: s.width, stroke: st.strike))
    })
  }
  // A document's own text colours must not make a deletion look inserted.
  show text: set text(fill: st.del)
  let struck = text(fill: st.del, strike(stroke: st.strike, body))
  // On a dark background, a light one of its own keeps it legible.
  context if redline-light-text() { highlight(fill: st.del-fill, struck) } else { struck }
}

#let redline-del-rules(body, st: redline-style) = redline-del-unlisted(redline-del-look(body, st: st))

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

#let redline-side-mark(kind, body, id: none) = {
  // `body` is a move's key for a move.
  let kind = if redline-side == "old" { if kind == "ins" { "del" } else { "ins" } } else { kind }
  redline-meta((kind, redline-hash(if kind == "ins" { 1 } else { 2 }, body), id), <redline-change>)
}

// A move is bracketed by tags of its own, because it encloses ordinary
// marks: "to" at the start of its new place and "end" at its end, which
// records its container as redline-tag does; "from" at the line at its old
// place. Start tags are found by their label. A tag next to a block-level
// element is not boxed, which would add a line; it then sits at the edge of
// the block instead of on a line of text.
#let redline-move-tag(kind, id, step: none, block: false) = {
  let v = (kind: kind, id: id, step: step, block: block)
  if kind != "end" {
    let tag = redline-meta(v, label("redline-move-" + kind + "-" + str(id)))
    if block { tag } else { box(tag) }
  } else {
    let tag = layout(size => redline-meta(v + (width: size.width, gutter: columns.gutter, columns: page.columns), <redline-move>))
    if block { place(tag) } else [\u{2060}#box(tag)]
  }
}

#let redline-move-find(kind, id) = query(label("redline-move-" + kind + "-" + str(id))).at(0, default: none)

// The page number printed on the page of `loc`, or its page in this PDF.
#let redline-page-of(loc) = if loc.page-numbering() == none { loc.page() } else { counter(page).at(loc).first() }

// Content as pieces that can be cut between: words of text, other elements
// whole.
#let redline-pieces(c) = if type(c) != content { (c,) } else if c.func() == [].func() {
  c.children.map(redline-pieces).flatten()
} else if c.func() == text and c.has("text") {
  c.text.split(" ").map(text).intersperse([ ])
} else { (c,) }

// The line at the old place of moved text, linked to its new place: the
// first words of what moved, as many as fit on one line. It takes the place
// of a paragraph, with the paragraph spacing above and below. Its arrow has
// the colour of deletions.
#let redline-move-line(m, note, step) = {
  parbreak()
  context {
    let spacing = if sys.version >= version(0, 12, 0) { par.spacing } else { 1.2em }
    block(above: spacing, below: spacing, width: 100%, layout(size => {
      set text(size: 0.85em, fill: redline-style.move, hyphenate: false)
      redline-move-tag("from", m.id, step: step)
      let pieces = redline-pieces(redline-neutral(note))
      let style = if m.heading { strong } else { emph }
      let to = redline-move-find("to", m.id)
      let head = if to == none [Moved:] else {
        let down = m.at("down", default: true)
        let where = if to.location().page() != here().page() [to p. #redline-page-of(to.location())] else if down [below] else [above]
        [#text(fill: redline-step-style(step).del, if down { sym.arrow.b } else { sym.arrow.t }) Moved #where:]
      }
      let counts = ((m.at("paragraphs", default: 0), "paragraph"), (m.at("items", default: 0), "item")).filter(c => c.at(0) > 0)
      let count = if counts.map(c => c.at(0)).sum(default: 0) + int(m.heading) > 1 {
        [ (#counts.map(((n, what)) => [#n #what#if n > 1 [s]]).join[ and ])]
      }
      let line(k) = {
        let words = if k > 0 { style(pieces.slice(0, k).join()) }
        [#head #words#if k < pieces.len() or m.at("more", default: false) [ …]#count]
      }
      let k = pieces.len()
      let full = measure(line(k)).width
      if full > size.width {
        // Words are about as wide as each other: start from the share that
        // fits, then step down.
        k = calc.floor(k * size.width / full)
        while k > 0 and (pieces.at(k - 1) == [ ] or measure(line(k)).width > size.width) { k -= 1 }
      }
      let line = redline-del-unlisted(line(k))
      if to == none { line } else { link(to.location(), line) }
      redline-restore(label("redline-move-from-" + str(m.id)))
    }))
  }
  parbreak()
}

#let redline-ins(body, block: false, step: none, move: none, id: none) = {
  if move != none {
    let end = move.at("end", default: false)
    // Both ends, so that the pages a long move spans are shown; the line at
    // the old place records two changes to match.
    let side = if redline-side != none { redline-side-mark("ins", move.key) }
    return side + redline-move-tag(if end { "end" } else { "to" }, move.id, step: step, block: move.block)
  }
  let st = redline-step-style(step)
  if redline-side != none { redline-side-mark("ins", body, id: id) }
  if redline-side == "old" {
    let look = redline-del-look.with(st: st)
    redline-mark("del", body, block, look, st.del-frame, frame-style: look, step: step, id: id)
  } else {
    redline-mark("ins", body, block, b => redline-ins-rules(b, block, st: st), st.ins-frame, frame-style: text.with(fill: st.ins), step: step, id: id)
  }
}

// On a side of the side-by-side layout, a deletion is content the version
// does not have: the page next to it shows it.
#let redline-del(body, block: false, heading: none, gap: false, step: none, move: none, id: none) = {
  let st = redline-step-style(step)
  if redline-side != none and move != none { return redline-side-mark("del", move.key) * 2 }
  if redline-side != none { return redline-side-mark("del", body, id: id) }
  if move != none { return redline-move-line(move, body, step) }
  let block = block or heading != none
  let body = redline-neutral(body)
  if block { parbreak() }
  let rules = redline-del-rules.with(st: st)
  let look = if heading == none { rules } else {
    b => rules(text(weight: "bold", size: if heading == 1 { 1.4em } else { 1.2em }, b))
  }
  // A label of its own, which is found faster than the last of many.
  let start = label("redline-del-start-" + repr(id))
  redline-meta(none, start)
  redline-mark("del", body, block, look, st.del-frame, frame-style: look, plain: redline-del-unlisted, step: step, id: id)
  redline-restore(start)
  if block { parbreak() } else if gap { h(0.25em) }
}

// The bars code shadows `top`, `bottom` and `left`. Bars are placed from the
// top left, also in right-to-left text.
#let redline-top-left = top + left
#let redline-bottom-left = bottom + left

#let redline-swatch(st) = box(width: 0.8em, height: 0.8em, fill: st.ins-fill, stroke: 0.6pt + st.ins)

#let redline-mono = text.with(font: "DejaVu Sans Mono", size: 0.85em)

// A revision label: a revision in monospace, a name such as "working tree"
// as text.
#let redline-rev(label) = if label.contains(" ") { label } else { redline-mono(label) }

// Each step's colour and label on one line no wider than `width`: with
// --each-commit the commit's hash and the start of its subject, hashes alone
// if subjects do not fit; with --steps the revision the step ends at. None
// if that does not fit.
#let redline-legend(hist, width, size: 7pt) = {
  let mono = redline-mono
  let items = if "groups" in hist {
    hist.groups.enumerate().map(p => (p.at(0) + 1, redline-rev(p.at(1).to), ()))
  } else {
    hist.commits.enumerate().map(p => (hist.earlier + p.at(0) + 1, mono(p.at(1).hash), p.at(1).subject.clusters()))
  }
  if hist.uncommitted and "groups" not in hist { items.push((hist.earlier + hist.commits.len() + 1, [uncommitted], ())) }
  let legend(n) = {
    set text(size: size, fill: luma(90), weight: "regular", style: "normal")
    items.map(((step, hash, subject)) => {
      let more = if subject.len() > n { "…" }
      box[#redline-swatch(redline-step-style(step))#h(0.35em)#hash#if n > 0 and subject.len() > 0 [ #subject.slice(0, calc.min(n, subject.len())).join().trim()#more]]
    }).join(h(1.2em))
  }
  let fits(n) = measure(legend(n)).width <= width
  let longest = calc.max(0, ..items.map(i => i.at(2).len()))
  if fits(longest) { return legend(longest) }
  let (lo, hi) = (0, longest)
  while lo < hi {
    let mid = calc.quo(lo + hi + 1, 2)
    if fits(mid) { lo = mid } else { hi = mid - 1 }
  }
  if lo >= 8 { legend(lo) } else if fits(0) { legend(0) }
}

// The bars of one page, drawn from the page foreground or background
// (`from`): one query per page instead of introspection per mark.
#let redline-page-bars(from) = context {
  let st = redline-style
  // Notes and the legend are laid out left to right, as the bars are.
  set text(dir: ltr)
  let pg = here().page()
  redline-meta(from, <redline-page>)
  let m = page.margin
  if pg == 1 {
    // For the summary and the binary: the pages with changes but without
    // bars. For the side-by-side layout also the number of pages, the ids of
    // the marks that were not laid out, each change's kind (0 deleted, 1
    // inserted), identity and page, and the same for each heading (kind 2),
    // whose identity is its text, all as one flat array of integers.
    let drawn = query(<redline-page>).map(m => m.location().page())
    let unbarred = query(selector(<redline-tag>).or(<redline-move>)).map(t => t.location().page()).dedup().filter(p => p not in drawn)
    redline-ints(unbarred, <redline-unbarred>)
    if redline-side != none {
      let changes = query(<redline-change>).map(m => (int(redline-value(m).at(0) == "ins"), redline-value(m).at(1), m.location().page()))
      let headings = query(heading).map(h => (2, redline-hash(3, h.body), h.location().page()))
      let lost = query(<redline-lost>).map(redline-value).flatten()
      redline-ints((drawn.len() + unbarred.len(), unbarred.len(), ..unbarred, lost.len(), ..lost, ..changes.flatten(), ..headings.flatten()), <redline-layout>)
    }
  }
  let tags = query(<redline-tag>)
  let n = calc.quo(tags.len(), 2)
  if n == 0 and query(<redline-move>).len() == 0 { return }
  let (w, h) = (page.width, page.height)
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
  // The x of a bar from tag `a` to end tag `b` on this page, `lane` lanes out,
  // whether it is in the right margin, and the x of the far side of the
  // free space there: the page edge or the text of the column before. A
  // container of width `cw` that is one of `cols` equal columns with the
  // gutter in effect, as page columns and `columns` are, has its bar just
  // left of it: in the margin or in the middle of the gutter, with the lanes
  // narrowed to fit. Any other container's left edge is unknown and text can
  // run up to it, so its bar goes in a margin: the right one if the
  // container cannot reach the left edge of the text area but can reach its
  // right edge, such as a right-hand cell. A container narrower than a page
  // column, such as a list item, is in one of the `n` page columns.
  let column(s, e, cw, g, n) = {
    let cw = if cw == none or cw <= 0pt { body } else { cw }
    let g = if g == none { 0pt } else { absolute(g, body) }
    let q = (body + g) / (cw + g)
    let cols = if calc.abs(q - calc.round(q)) < 0.001 { int(calc.round(q)) } else { 1 }
    if cols == 1 and n != none and n > 1 {
      cols = n
      cw = (body - (n - 1) * g) / n
    }
    let k = calc.clamp(calc.floor(((if s.page == pg { s.x } else { e.x }) - left) / (cw + g)), 0, cols - 1)
    (k, cols, cw, g)
  }
  let bar-x(s, e, cw, g, n, lane) = {
    let (k, cols, cw, g) = column(s, e, cw, g, n)
    let at = calc.min(s.x, e.x) - left
    if cols > 1 and k > 0 {
      let l = calc.max(0pt, calc.min(st.lane, (g - st.bar-width) / 2))
      let edge = left + k * (cw + g) - g
      (calc.max(left + k * (cw + g) - calc.min(st.bar-gap, g / 2) + l / 2 - lane * l, edge + 3pt), false, edge)
    } else if cols == 1 and at > cw and at + cw > body - 1pt {
      (calc.min(left + body + calc.min(st.bar-gap, right / 2) + lane * st.lane, w - st.bar-width / 2), true, w)
    } else {
      (calc.max(left - calc.min(st.bar-gap, left / 2) - lane * st.lane, st.bar-width / 2), false, 0pt)
    }
  }
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
    let (a, b) = (redline-value(a), redline-value(b))
    // A mark ending above the text area is in a header or title bar.
    if e.page > pg or e.y > top {
      let y0 = if s.page == pg { calc.max(s.y + a.at(1).to-absolute(), top - 1em.to-absolute()) } else { top }
      let y1 = if e.page == pg { e.y + b.at(1).to-absolute() } else { bottom }
      let ins = a.at(0) == "ins"
      // Deletions are one lane further out.
      let (x, ..) = bar-x(s, e, b.at(2, default: none), b.at(3, default: none), b.at(5, default: none), if ins { 0 } else { 1 })
      let sst = redline-step-style(a.at(4, default: none))
      bars.push((x, y0 + 1pt, y1 - 1pt, ins, if ins { sst.ins } else { sst.del }))
    }
    i += 1
  }
  // Overlapping bars of one kind and colour in one place are drawn as one,
  // so that dashes do not overlap.
  let merged = ()
  for bar in bars.sorted(key: b => (b.at(0).pt(), b.at(1).pt())) {
    let last = if merged.len() > 0 { merged.last() }
    if last != none and last.at(0) == bar.at(0) and last.slice(3) == bar.slice(3) and bar.at(1) <= last.at(2) + 2pt {
      merged.last().at(2) = calc.max(last.at(2), bar.at(2))
    } else { merged.push(bar) }
  }
  for (x, y0, y1, ins, paint) in merged {
    let stroke = (paint: paint, thickness: st.bar-width, dash: if ins { none } else { (3pt, 2pt) })
    place(redline-top-left, dx: x, dy: y0, line(angle: 90deg, length: calc.max(y1 - y0, 0pt), stroke: stroke))
  }
  // A move's bracket, from the start of its new place to its end, linked to
  // its old place, with the x of the far side of the free space for its
  // note. In the first column it is in a lane outside the bars, in the last
  // of several in the right margin, and in a column between them close to
  // the text, inside the bars, without a note: the gutter is the other
  // column's too. End tags come in document order, each after its start tag.
  let bracket-x(s, e, cw, g, n) = {
    let (k, cols, cw, g) = column(s, e, cw, g, n)
    if cols > 1 and k == cols - 1 {
      let x = left + body + calc.min(st.bar-gap, right / 2) / 2
      (x, true, w)
    } else if cols > 1 and k > 0 {
      let x = left + k * (cw + g) - calc.min(3.5pt, g / 4)
      (x, false, x - 5pt)
    } else { bar-x(s, e, cw, g, n, 2.5) }
  }
  let ends = query(<redline-move>)
  let (lo, hi) = (0, ends.len())
  while lo < hi {
    let mid = calc.quo(lo + hi, 2)
    if ends.at(mid).location().page() < pg { lo = mid + 1 } else { hi = mid }
  }
  for t in ends.slice(lo) {
    let v = redline-value(t)
    let a = redline-move-find("to", v.id)
    if a == none { continue }
    let (s, e) = (a.location().position(), t.location().position())
    if s.page > pg { break }
    // Brackets of moves that follow each other stay a little apart.
    let y0 = if s.page < pg { top } else { s.y - (if redline-value(a).block { 0.1em } else { 0.8em }).to-absolute() + 2.5pt }
    let y1 = if e.page > pg { bottom } else { e.y + (if v.block { 0em } else { 0.3em }).to-absolute() - 2.5pt }
    let stroke = st.move-stroke + st.move
    // The end of a line of text can reach past the column; its column is
    // the one it ends.
    let e-in = if v.block { e } else { (page: e.page, x: e.x - 2pt, y: e.y) }
    // A move from one column into the next is bracketed in both.
    let x-of(p, q) = bracket-x(p, q, v.width, v.gutter, v.at("columns", default: none))
    let (xs, xe) = (x-of(s, s), x-of(e-in, e-in))
    let pieces = if s.page == pg and e.page == pg and xs.at(0) != xe.at(0) {
      ((xs, y0, bottom, true, false), (xe, top, y1, false, true))
    } else { ((x-of(s, e-in), y0, y1, s.page == pg, e.page == pg),) }
    for ((x, right-side, _), from, to, first, last) in pieces {
      let tick = if right-side { -2.5pt } else { 2.5pt }
      place(redline-top-left, dx: x, dy: from, line(angle: 90deg, length: calc.max(to - from, 0pt), stroke: stroke))
      if first { place(redline-top-left, dx: x, dy: from, line(length: tick, stroke: stroke)) }
      if last { place(redline-top-left, dx: x, dy: to, line(length: tick, stroke: stroke)) }
    }
    let ((x, right-side, edge), _, y1, ..) = pieces.first()
    let (x0, x1) = if right-side { (x - 3pt, edge - 2pt) } else { (edge + 2pt, x + 3pt) }
    let other = redline-move-find("from", v.id)
    if other != none {
      place(redline-top-left, dx: x0, dy: y0, link(other.location(), box(width: calc.max(x1 - x0, 0pt), height: calc.max(y1 - y0, 1pt))))
    }
    if s.page != pg { continue }
    let notes = if other != none and other.location().page() != pg {
      let n = redline-page-of(other.location())
      let arrow = if other.location().page() < pg { sym.arrow.t } else { sym.arrow.b }
      ([moved from p. #n #arrow], [moved#linebreak()from p. #n #arrow], [moved])
    } else { ([moved],) }
    let free = if right-side { edge - 2pt - x - 3pt } else { x - 3pt - edge - 2pt }
    let sst = redline-step-style(redline-value(a).step)
    let style = text.with(size: 6.5pt, fill: if redline-side == "old" { sst.del } else { sst.ins }, top-edge: "cap-height", bottom-edge: "baseline", hyphenate: false)
    let note = notes.map(n => { set par(leading: 0.4em); style(n) }).find(n => measure(n).width <= free)
    if note != none {
      let note = if other != none { link(other.location(), note) } else { note }
      place(redline-top-left, dx: if right-side { x + 3pt } else { edge + 2pt }, dy: y0, box(width: free, align(if right-side { start } else { end }, note)))
    }
  }
  // The legend goes in the middle of the bottom margin below the footer,
  // if there is room. A page number is taken to be one line high; in
  // Typst 0.11 an unset footer reads as none.
  let hist = query(<redline-history>)
  let legend = if merged.len() > 0 and hist.len() > 0 { redline-legend(redline-value(hist.first()), body) }
  if legend != none {
    let margin = side(("bottom",), h)
    let foot = if page.footer not in (none, auto) { page.footer } else if page.numbering != none and page.number-align.y == redline-bottom-left.y [0]
    let below = if foot == none { 0pt } else {
      absolute(page.footer-descent, margin) + measure(block(width: body, foot)).height
    }
    let room = margin - below - measure(legend).height
    if room >= 6pt {
      place(redline-bottom-left, dx: left, dy: -room / 2, legend)
    }
  }
}

// The rest of the document, with the bars drawn over the page foreground in
// effect here. A page whose foreground the document replaces later gets them
// under its background instead; the page background is drawn below the
// content, but the bars are in the margin.
#let redline-bars(body) = context {
  let (fg, bg) = (page.foreground, page.background)
  // The foreground of a page comes after its background: the first bars
  // drawn from a foreground after the background are this page's if its
  // foreground drew them. Not a query of all pages, which would cost time
  // with the square of their number.
  let fallback = context {
    let next = query(selector(<redline-page>).after(here())).find(m => redline-value(m) == "fg")
    if next == none or next.location().page() != here().page() { redline-page-bars("bg") }
  }
  set page(foreground: { fg; redline-page-bars("fg") }, background: { bg; fallback })
  // Marks in hidden content get no bars and count as not laid out. A query
  // finds an element that a show rule replaced, so the visible tags are
  // the ones labelled here.
  // The visible tag repeats the value that the tag set just before it.
  show <redline-tag-at>: t => [#state("redline").update(v => v)<redline-tag>]
  show hide: it => {
    show <redline-tag-at>: none
    it
  }
  body
}

// The bibliography entries and other files that changed, as sentences, or
// none.
#let redline-bib(bib) = {
  let entries = (([new], bib.added), ([no longer cited], bib.removed), ([changed], bib.changed)).filter(p => p.at(1).len() > 0)
  let files = bib.at("files", default: ())
  let out = ()
  if entries.len() > 0 { out.push[Bibliography entries, not marked in the bibliography: #entries.map(p => [#p.at(0): #p.at(1).map(raw).join(", ")]).join("; ").] }
  if files.len() > 0 { out.push[Changed files that the document reads, not marked: #files.map(raw).join(", ").] }
  if bib.at("unchecked", default: false) { out.push[Files that the document reads through a computed path were not checked for changes; that needs Typst 0.14 or later.] }
  if out.len() > 0 { out.join(" ") }
}

// The steps of --steps, each in the colour of its marks with its range,
// the date of its last commit, its number of commits and the subjects of
// the first few.
#let redline-groups(hist, old, new) = {
  let shown = 4
  let start = 0
  let rows = ()
  for (i, g) in hist.groups.enumerate() {
    let st = redline-step-style(i + 1)
    let commits = hist.commits.slice(start, start + g.commits)
    start += g.commits
    let uncommitted = hist.uncommitted and i == hist.groups.len() - 1
    let count = if g.commits > 0 [#g.commits #if g.commits == 1 [commit] else [commits]]
    let range = [#redline-rev(g.from)..#redline-rev(g.to)]
    rows += (redline-swatch(st), ..(range, commits.at(-1, default: (date: [])).date, count).map(text.with(fill: st.ins)))
    let listed = if commits.len() > shown { commits.slice(0, shown - 1) } else { commits }
    let lines = listed.map(c => [#redline-mono(c.hash) #c.subject])
    if listed.len() < commits.len() { lines.push[and #(commits.len() - listed.len()) more] }
    if uncommitted { lines.push(emph[Uncommitted changes]) }
    if lines.len() == 0 { lines.push(emph[No changes to the document]) }
    rows += ([], grid.cell(colspan: 3, text(size: 0.85em, lines.join(linebreak()))))
  }
  block(below: 1.5em, {
    block(below: 0.8em, strong[Steps from #old to #new])
    set par(justify: false)
    grid(columns: (auto, auto, auto, 1fr), column-gutter: 0.6em, row-gutter: 0.6em, ..rows)
  })
}

// The commits from the old version to the new one, each in the colour of
// its marks if marks are coloured by commit; none without a history.
#let redline-commits(hist, old, new) = if hist != none and "groups" in hist {
  redline-meta(hist, <redline-history>)
  redline-groups(hist, old, new)
} else if hist != none {
  let row(step, ..cells) = if hist.steps {
    let st = redline-step-style(step)
    (redline-swatch(st), ..cells.pos().map(text.with(fill: st.ins)))
  } else { cells.pos() }
  let rows = hist.commits.enumerate().map(p => row(hist.earlier + p.at(0) + 1, raw(p.at(1).hash), p.at(1).date, p.at(1).subject))
  if hist.uncommitted { rows.push(row(hist.earlier + hist.commits.len() + 1, [], [], emph[Uncommitted changes])) }
  if hist.steps { redline-meta(hist, <redline-history>) }
  block(below: 1.5em, {
    block(below: 0.8em, strong[Commits from #old to #new])
    if hist.earlier > 0 { block(emph[#hist.earlier earlier commits, not listed]) }
    grid(columns: if hist.steps { (auto, auto, auto, 1fr) } else { (auto, auto, 1fr) }, column-gutter: 0.6em, row-gutter: 0.6em, ..rows.flatten())
  })
}

// Changed code that is not marked, (old, new): the old code in the colour
// of deletions, the new in that of insertions. Text, not raw, so that the
// document's show rules on raw leave it alone.
#let redline-code(c) = {
  let st = redline-style
  let rows = ((c.old, "−", st.del), (c.new, "+", st.ins)).filter(r => r.first() != "")
  set text(font: "DejaVu Sans Mono", size: 0.9em, hyphenate: false)
  set par(justify: false)
  grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, ..rows.map(((code, sign, fill)) => (text(fill: fill, sign), text(fill: fill, code))).flatten())
}

#let redline-move-bracket = {
  let s = redline-style.move-stroke + redline-style.move
  box(width: 4pt, height: 0.65em, {
    place(dx: 0.5pt, line(angle: 90deg, length: 0.65em, stroke: s))
    place(dx: 0.5pt, line(length: 2.5pt, stroke: s))
    place(dx: 0.5pt, dy: 0.65em, line(length: 2.5pt, stroke: s))
  })
}

#let redline-move-legend(st) = [Moved text is shown once, unmarked at its new place, with a grey bracket #redline-move-bracket and a #text(fill: st.ins, size: 0.8em)[moved] note in the margin; at its old place a #text(fill: redline-style.move)[grey line] with an arrow #text(fill: st.del, sym.arrow.b) names its first words and links to it. Edits made while moving are marked as usual.]

// The start tags of the marks laid out; tags come in pairs.
#let redline-starts() = {
  let tags = query(<redline-tag>)
  range(0, tags.len(), step: 2).map(i => tags.at(i))
}

// The ids of the marks laid out, as keys.
#let redline-laid-out() = {
  let ids = (:)
  let at(tags, k) = tags.map(t => redline-value(t).at(k, default: none))
  for i in at(redline-starts(), 6) + at(query(<redline-change>), 2) {
    if i != none { ids.insert(str(i), true) }
  }
  ids
}

// The marks among `marks` that were not laid out (`shown` is from
// `redline-laid-out`), such as marks in content that a function reads as
// text, listed as code; a move counts as laid out if its new place is.
#let redline-lost(marks, shown) = {
  let lost = marks.filter(m => if "move" in m {
    query(label("redline-move-to-" + str(m.move))).len() == 0
  } else { str(m.id) not in shown })
  if lost.len() > 0 { redline-ints(lost.map(m => m.id), <redline-lost>) }
  if lost.len() > 0 and redline-side == none {
    pad(left: 1.5em, text(size: 0.85em, {
      block(text(fill: redline-style.del)[Changes that did not reach the page, not marked:])
      lost.map(redline-code).join()
    }))
  }
}

// Page numbers in increasing order as runs, such as "2–4, 7".
#let redline-pages(ps) = {
  let runs = ()
  for p in ps {
    if runs.len() > 0 and runs.last().last() + 1 == p { runs.last().push(p) } else { runs.push((p,)) }
  }
  // `join` takes no default before Typst 0.12.
  if runs.len() == 0 { "–" } else { runs.map(r => str(r.first()) + if r.len() > 1 { "–" + str(r.last()) } else { "" }).join(", ") }
}

// A line of the summary that links to the element `el` with `title`, its
// number if it is a numbered heading, `what` changed, and its page.
#let redline-section-line(el, title, what) = {
  let loc = el.location()
  let num = if el.func() == heading and el.numbering != none {
    numbering(el.numbering, ..counter(heading).at(loc)) + h(0.5em)
  }
  [#link(loc, underline[#num#title]) (#what)#box(width: 1fr, repeat[#h(0.4em).])#h(0.4em)#link(loc, [#redline-page-of(loc)])]
}

// The text of content without its deletions, whose marks must not be laid
// out again.
#let redline-plain(c) = if type(c) == str { c } else if type(c) != content { "" } else if c.func() in (strike, footnote, metadata) { "" } else if c.has("text") and type(c.text) == str { c.text } else if c.func() == [ ].func() { " " } else if c.has("children") {
  c.children.map(redline-plain).join()
} else if c.has("body") { redline-plain(c.body) } else if c.has("child") { redline-plain(c.child) } else { "" }

// The marks among `marks` that are laid out, grouped by the last heading of
// level 1 or 2 before them, or none: array of (heading, count: marks other
// than moves, moves: numbers of the moves, at: locations). Deleted headings,
// which are neither outlined nor bookmarked, do not count.
#let redline-under-headings(marks) = {
  let at = (:)
  for t in redline-starts() {
    let id = redline-value(t).at(6)
    if id != none and str(id) not in at { at.insert(str(id), t.location()) }
  }
  let kept(x) = x.level <= 2 and (x.at("outlined", default: true) or x.at("bookmarked", default: auto) != false)
  let groups = ()
  for m in marks {
    let loc = if "move" in m {
      let to = redline-move-find("to", m.move)
      if to != none { to.location() }
    } else { at.at(str(m.id), default: none) }
    if loc == none { continue }
    let head = query(selector(heading).before(loc)).filter(kept).at(-1, default: none)
    let i = groups.position(g => if head == none { g.heading == none } else { g.heading != none and g.heading.location() == head.location() })
    if i == none {
      groups.push((heading: head, count: 0, moves: (), at: ()))
      i = groups.len() - 1
    }
    groups.at(i).at.push(loc)
    if "move" in m { groups.at(i).moves.push(m.move) } else { groups.at(i).count += 1 }
  }
  groups
}

#let redline-summary(info) = if redline-side == none {
  let st = redline-style
  // Pages of its own, without the document's header, footer and numbers.
  pagebreak(weak: true)
  set page(header: none, footer: none, numbering: none)
  context {
    let shown = redline-laid-out()
    // English, whatever language the document is in.
    set text(lang: "en", dir: ltr)
    block({
      block(text(size: 1.4em, weight: "bold")[Changes from #info.old to #info.new])
      let bib = redline-bib(info.at("bib", default: (added: (), removed: (), changed: ())))
      let steps = info.at("history", default: (steps: false)).steps
      if info.entries.len() == 0 and bib == none { [No changes.] } else if steps {
        let st = st + redline-colours(luma(70))
        let frame(stroke, body) = box(stroke: stroke, inset: (x: 2pt), outset: (y: 2pt), body)
        block(below: 1.5em)[#redline-ins-rules([Inserted], false, st: st) and #redline-del-rules(st: st)[deleted] text, in the colour of the #if "groups" in info.history [step] else [commit] that made the change. New equations, figures, code blocks and drawings are #frame(st.ins-frame)[framed], deleted ones #frame(st.del-frame)[framed dashed] and crossed where they cannot be struck through; code blocks and filled boxes keep their colours. Bars in the margin: solid for inserted lines, dashed and further out for deleted ones. #if info.at("moved", default: 0) > 0 [#redline-move-legend(st)] Numbers count changed passages.]
      } else {
        let frame(stroke, body) = box(stroke: stroke, inset: (x: 2pt), outset: (y: 2pt), body)
        block(below: 1.5em)[#redline-ins-rules([Inserted], false) and #redline-del-rules[deleted] text; #text(fill: st.ins)[new paragraphs] in blue. New equations, figures, code blocks and drawings are #frame(st.ins-frame)[framed], deleted ones #frame(st.del-frame)[framed dashed] and crossed where they cannot be struck through; code blocks and filled boxes keep their colours. Bars in the margin: solid for inserted lines, dashed and further out for deleted ones. #if info.at("moved", default: 0) > 0 [#redline-move-legend(st)] Numbers count changed passages.]
      }
      redline-commits(info.at("history", default: none), info.old, info.new)
      if bib != none { block(below: 1.5em, bib) }
      if redline-fallback-note != none { block(below: 1.5em, text(fill: st.del, redline-fallback-note)) }
      context {
        let unbarred = query(<redline-unbarred>).map(redline-value).flatten()
        if unbarred.len() > 0 {
          block(text(fill: st.del)[Changes on these pages of this PDF have no bars in the margin, because the document sets its own page foreground and background there: #unbarred.map(str).join(", ").])
        }
        let lost = query(<redline-unrestored>)
        if lost.len() > 0 {
          let keys = lost.map(redline-value).flatten().dedup()
          let pages = lost.map(m => m.location().page()).dedup()
          let names = keys.map(k => if k == page [the page counter] else { raw(k) }).join(", ")
          let (it, that) = if keys.len() == 1 { ([a state], [that state]) } else { ([states], [those states]) }
          let on = if pages.len() == 1 [page] else [pages]
          block(below: 1.5em, text(fill: st.del)[Deleted content on #on #pages.map(str).join(", ") updates #it (#names). What reads #that afterwards can differ from the new version.])
        }
      }
      let min-level = calc.min(..info.entries.map(e => e.level), 9)
      let row(level, line, e) = block(above: 0.5em, below: 0.5em, pad(left: (level - min-level) * 1.5em, {
        line
        if e.code.len() > 0 { pad(left: 1.5em, text(size: 0.85em, e.code.map(redline-code).join())) }
        redline-lost(e.marks, shown)
      }))
      for e in info.entries {
        let note = if e.code.len() > 0 [; changed code not marked]
        let what(count, moves, ..label) = (..label.pos(), if count > 0 [#count], if moves > 0 [#moves moved]).filter(x => x != none).join[, ]
        let moves = e.at("moves", default: 0)
        if e.deleted {
          row(e.level, text(fill: st.del)[#strike(e.title) (deleted section#note)], e)
        } else if e.at("before", default: false) {
          // Headings that the source does not have at its top level, such
          // as those inside calls, are found where they are laid out.
          context {
            let groups = redline-under-headings(e.marks)
            let found = groups.filter(g => g.heading != none)
            let placed = found.map(g => g.count).sum(default: 0)
            let left = calc.max(0, moves - found.map(g => g.moves).flatten().dedup().len())
            if e.count > placed or left > 0 {
              let at = groups.filter(g => g.heading == none).map(g => g.at).flatten()
              let leader = if at.len() > 0 [#box(width: 1fr, repeat[#h(0.4em).])#h(0.4em)#redline-pages(at.map(redline-page-of).dedup())]
              row(e.level, [#e.title (#what(e.count - placed, left)#note)#leader], e)
            }
            let top = calc.min(..found.map(g => g.heading.level), 9)
            for g in found {
              row(e.level + g.heading.level - top, redline-section-line(g.heading, redline-plain(g.heading.body).trim(), what(g.count, g.moves.dedup().len())), (code: (), marks: ()))
            }
          }
        } else if e.target == none {
          row(e.level, [#e.title (#what(e.count, moves)#note)], e)
        } else {
          let new = if e.new { ([new section],) } else if e.at("moved", default: false) { ([moved section],) } else { () }
          row(e.level, context redline-section-line(query(e.target).first(), e.title, [#what(e.count, moves, ..new)#note]), e)
        }
      }
    })
  }
} else { context redline-lost(info.entries.map(e => e.marks).flatten(), redline-laid-out()) }

// The side-by-side layout. `info` is (old, new: (label, pdf: the version
// compiled with `redline-side`, pages, unbarred: pages with changes but no
// bars), pairs: array of (old, new: pages
// shown next to each other, steps-old, steps-new: pages of earlier steps of
// the same slide, not shown), unpaired: array of (side, ins, page): changes
// found in one version only, unmarked: array of (file, old, new): changes
// that are not marked, bib: as for `redline-summary`, moved: whether text
// moved).
#let redline-side-by-side(info) = context {
  let st = redline-style
  let (o, n) = (info.old, info.new)
  let (wo, wn) = (measure(image(o.pdf, page: 1)).width, measure(image(n.pdf, page: 1)).width)
  let width = wo + wn
  // Pages are shown at their own size, so text is sized to them: about the
  // size of body text on a page that wide.
  let size = calc.max(calc.max(wo, wn) / 50, 10pt)
  let gap = 2 * size
  let pages = redline-pages
  let shown(ps, steps) = if ps.len() == 0 [–] else [#pages(ps)#if steps.len() > 0 [ (last step of #pages(steps + ps))]]
  let name(side) = text(weight: "bold", fill: if side == o { st.del } else { st.ins }, side.label)
  let left-out(side, ps) = side.pages - ps.map(p => p.len()).sum(default: 0)
  let hist = info.at("history", default: none)
  let steps = hist != none and hist.steps
  let sample = if steps { st + redline-colours(luma(70)) } else { st }
  set page(width: auto, height: auto, margin: gap)
  set text(size: size)
  block(width: width + gap, align(center, block(width: calc.min(width + gap, 36 * size), align(left, {
    block(text(size: 1.4em, weight: "bold")[Changes from #o.label to #n.label])
    let (lo, ln) = (left-out(o, info.pairs.map(p => p.old + p.steps-old)), left-out(n, info.pairs.map(p => p.new + p.steps-new)))
    let bib = redline-bib(info.bib)
    if info.pairs.len() == 0 { if bib == none and info.unmarked.len() == 0 [No changes.] } else [
      Pages of #name(o) on the left, with #redline-del-rules(st: sample)[deleted] text; pages of #name(n) on the right, with #redline-ins-rules([inserted], false, st: sample) text#if steps [, in the colour of the #if "groups" in hist [step] else [commit] that made the change]. Bars in the margin mark the changed lines.#if info.at("moved", default: false) [ Moved text has a grey bracket #redline-move-bracket and a #text(size: 0.8em)[moved] note in the margin on both sides; edits made while moving are marked as usual.]
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
      [Changes that are not marked:]
      list(..info.unmarked.map(u => [In #raw(u.file): #redline-code(u)]))
    }
    if bib != none { block(bib) }
    if redline-fallback-note != none { block(text(fill: st.del, redline-fallback-note)) }
    redline-commits(hist, o.label, n.label)
    for side in (o, n) {
      if side.unbarred.len() > 0 { block[Changes on these pages of #name(side) have no bars in the margin, because the document sets its own page foreground and background there: #pages(side.unbarred).] }
    }
  }))))
  let head(side, pg, s, ps) = if pg != none [#name(side), page #pg#if s.len() > 0 [, the last step of #pages(s + (pg,))]] else if ps.len() == 0 [Not in #name(side)]
  // A side without a page keeps its width, so that the other stays in place.
  let shot(side, w, pg) = if pg == none { box(width: w) } else { box(stroke: 0.5pt + luma(150), image(side.pdf, page: pg)) }
  let legend = if steps { redline-legend(hist, width + gap, size: 0.8 * size) }
  for p in info.pairs {
    for r in range(calc.max(p.old.len(), p.new.len())) {
      pagebreak()
      let (po, pn) = (p.old.at(r, default: none), p.new.at(r, default: none))
      grid(columns: 2, column-gutter: gap, row-gutter: 0.5em, head(o, po, p.steps-old, p.old), head(n, pn, p.steps-new, p.new), shot(o, wo, po), shot(n, wn, pn), ..if legend != none { (grid.cell(colspan: 2, align(center, legend)),) })
    }
  }
}
