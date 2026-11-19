// typst-redline: how changes look. To restyle, pass a file of your own with
// --preamble. The marked files import four names from it:
// - redline-ins(body, block: false): inserted content; `block` when it is a
//   whole paragraph, item, heading or code expression.
// - redline-del(body, block: false, heading: none, item: none, gap: false):
//   deleted content; `heading` is the level of a deleted heading, `gap` says
//   that an insertion follows. A deleted list or enum item is a block with
//   `item`: (depth: its nesting level, number: its number in the old
//   enumeration or none in a list, counted: whether that number counts from
//   the enumeration's start, inside: whether it is written in the body
//   of the item before it, tight: whether its list is tight). A deleted
//   block is written as a continuation of the block before it, so a block
//   deletion must start its own paragraph.
// - Moved text: `redline-del(move: m)[first words]` stands at its old place,
//   and `redline-ins(move: m)[]` at the start and, with `m.end`, at the end
//   of its new place, which holds it unmarked or with its edits marked.
//   `m` is (id: the move in this document, key: the same for the move back,
//   heading: the passage starts with a heading, down: the new place comes
//   later in the document, more: the passage goes on after the first words,
//   block: the marker stands next to a block-level element or on a line of
//   its own).
// - redline-bars(body): the rest of the document, which draws change bars on
//   its pages (`body => body` for nothing).
// - redline-summary(info): the summary, after the document; `info` is (old, new: version labels,
//   ins, del: numbers of changes, moved: number of moves, entries: array of
//   (file, level, target: label or none, title, count: changes, a deletion
//   and the insertion that replaces it counting as one,
//   moves: moves into or out of the section other than its own, before:
//   the changes before the first heading at the top level, deleted,
//   new, moved, code: array of (id, old, new): changed code that is not
//   marked, either empty if inserted or deleted, bars: array of (id, old,
//   new, what): marks that show only a bar, said in `what`, marks: array of (id,
//   move: number of the move if it is one, from: true at its old place,
//   joined: true if it continues the mark before it, old, new, what: for
//   replaced code, its changed arguments as in `bars`): its marks), bib:
//   (added, removed, changed: arrays of citation keys whose bibliography entry is new, gone
//   or edited, files: paths of other files that the document reads and
//   that differ, unchecked: whether files read through a computed path were
//   not checked, names: the entries of those keys by authors and year, such
//   as "Aguado et al. 2017", by key, where found), and if the versions are
//   git revisions,
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
// were not laid out and to number the changes.
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
    // Frames of block-level elements, this far outside them. A block ends
    // at the baseline of its last line: the frame clears the descenders.
    ins-frame: 0.6pt + ins,
    del-frame: (paint: del, thickness: 0.6pt, dash: "dashed"),
    outset: (x: 0.4em, top: 0.3em, bottom: 0.5em),
    // Change bars sit this far outside the text area, at most half the
    // margin; deletion bars one lane further out, so that the two never
    // touch.
    bar-gap: 0.5cm,
    bar-width: 2pt,
    del-bar-width: 1.5pt,
    lane: 3.5pt,
    // Brackets and lines of moved text.
    move: luma(110),
    move-stroke: 0.8pt,
    // The numbers of changes, in the margin and on the summary.
    number: luma(110),
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

// The tool's own bookkeeping: an update of the state "redline", which the
// document's queries do not find as they find metadata, and an empty
// labelled `hide` after it, which takes no room. The label is not on the
// update, because functions that rebuild content from its fields can
// construct a `hide` but not a state update. `redline-value` reads the value
// back. `redline-ints`, for the summary, is a labelled update whose key,
// which the binary reads, lists the integers.
#let redline-meta(value, label) = [#state("redline", none).update(value)#hide([])#label]
#let redline-value(m) = state(m.at("key", default: "redline")).at(m.location())
#let redline-ints(ints, label) = [#state(if ints.len() == 0 { "" } else { ints.map(str).join(",") }, none).update(ints)#label]

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
// deleted content and sets no counter back after it, "frames" frames no
// content computed at layout, "names" shows deleted code by name, "whole"
// marks changed code whole.
#let redline-fallback = sys.inputs.at("redline-fallback", default: "").split(",")

// What the fallbacks show differently, for the summary; none without any.
#let redline-fallback-note = {
  let says = (
    states: [Numbers after deleted content can differ from the new version, because counters that it changes are not set back.],
    frames: [Changed drawings, and other content that the document lays out by code, have no frame; the bar in the margin still marks them.],
    names: [Deleted code is shown by its name instead of as it looked.],
    whole: [A changed call is marked whole, the old call deleted and the new one inserted, even where only text inside it changed.],
  )
  let lines = redline-fallback.filter(f => f in says).map(f => says.at(f))
  if lines.len() > 0 [Some changes are marked more coarsely, because the document did not compile with every change marked in full. #lines.join[ ]]
}

// Whether a part of a mark is laid out as a block of its own and gets a
// frame. An image or a shape is, and content computed at layout (a drawing)
// is when it is all of a block mark. Names, because some shapes exist only
// in some Typst versions. Content computed at layout can also set the page,
// which no frame can hold, so the "frames" fallback leaves it unframed.
#let redline-framed(c, sole) = (
  c.func() in (figure, block, align, grid, table, stack, pad, columns)
    or c.func() in (math.equation, raw) and c.block
    or repr(c.func()) in ("image", "rect", "square", "circle", "ellipse", "polygon", "line", "path", "curve")
    or sole and repr(c.func()) in ("context", "layout") and "frames" not in redline-fallback
)

// The two lines that cross out deleted content of size `w` by `h`, light
// enough that the content stays legible, on a white edge that shows them
// on a fill of their own colour.
#let redline-cross(stroke, w, h) = {
  for s in (1.5pt + white.transparentize(45%), 0.75pt + stroke.paint.transparentize(30%)) {
    place(top + left, line(start: (0pt, 0pt), end: (w, h), stroke: s))
    place(top + left, line(start: (0pt, h), end: (w, 0pt), stroke: s))
  }
}

// A frame around a block-level element, outside it so that nothing moves;
// as wide as the element. It carries the mark's tags when the element
// starts or ends the mark, so that the bar spans the frame. Deleted content
// that is not struck is `cross`ed if it is a picture high enough for the
// cross to stand clear, or else, like other such content, `tint`ed, so
// that text in it stays legible.
// A deleted element (`whole`) that fits on a page is not split across
// pages, where the frame would hold only its last part.
#let redline-frame(it, stroke, cross: false, tint: false, whole: false, start: none, end: none, step: none, id: none) = layout(region => block(width: 100%, breakable: not whole or measure(block(width: region.width, it)).height > region.height, layout(size => {
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
  // A figure's caption can be wider than its body: the frame holds the
  // figure as laid out in this width, its caption wrapped.
  let laid = if sys.version >= version(0, 12, 0) { measure(it, width: size.width).width } else { natural.width }
  let body-w = if type(rel) in (ratio, relative) { w } else { calc.min(measure(if it.func() == figure { it.body } else { it }, ..room).width, w) }
  let w = if it.func() == figure { calc.min(calc.max(w, laid), size.width) } else { w }
  let a =if it.func() == align { it.alignment } else if eq or it.func() == figure { center } else { align.alignment }
  let a = if a.axis() == "horizontal" { a } else if a.axis() == none { a.x } else { start }
  it
  // Content of no height, such as an empty header, gets no frame.
  let empty = h == 0pt and repr(it.func()) != "line"
  // The cross covers a figure's body and leaves its caption, which is
  // struck, alone.
  let (cw, ch) = if it.func() == figure { (body-w, measure(block(width: size.width, it.body)).height) } else { (w, h) }
  let cross = cross and not empty and ch >= 0.3 * cw
  let fill = if (cross or tint) and not cross { stroke.paint.transparentize(90%) }
  place(top + a, box(width: w, height: h, {
    if not empty { place(rect(width: 100%, height: 100%, stroke: stroke, fill: fill, outset: st.outset)) }
    if cross and it.func() == figure {
      let caption = it.at("caption", default: none)
      let above = caption != none and caption.at("position", default: figure.caption.position) == top
      place(center, dy: if above { h - ch } else { 0pt }, box(width: cw, height: ch, redline-cross(stroke, cw, ch)))
    } else if cross { redline-cross(stroke, w, h) }
    place(top + left, if start != none { redline-tag(start, -st.outset.top, step: step, id: id) })
    place(bottom + left, if end != none { redline-tag("end", st.outset.bottom, width: size.width) })
  }))
})))

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

// Whether content draws nothing but does something, such as a counter
// update or metadata, between spaces.
#let redline-unseen(c) = type(c) == content and (
  c.func() == metadata
    or repr(c.func()) in ("counter-update", "state-update")
    or c.func() == [].func() and c.children.any(redline-unseen) and c.children.all(x => redline-unseen(x) or x.func() in ([ ].func(), parbreak))
    or repr(c.func()) == "styled" and redline-unseen(c.child)
)

// Whether content shows colours of its own that a mark's colour or strike
// would make hard to read: a raw block, or an element with a fill.
#let redline-own-colours(c) = type(c) == content and (
  c.func() == raw and c.block
    or c.func() not in (text, highlight) and c.at("fill", default: none) != none
    or c.fields().values().any(v => redline-own-colours(v) or type(v) == array and v.any(redline-own-colours))
)

// Placed after the first part `c` of block mark `id`, which can start with a
// page break, as a slide or a heading shown on a new page does; the mark's
// start tag then stays at the end of the page before, where the mark has no
// content. Records whether `c` is a heading, which does not break across
// pages, and whether this is at the top of the text area, after a part that
// only broke the page.
#let redline-first(c, id) = context {
  let (m, w, h) = (page.margin, page.width, page.height)
  let v = if type(m) == dictionary { m.at("top", default: m.at("y", default: m.at("rest", default: auto))) } else { m }
  let full = if h == auto { 0pt } else { h }
  let top = if v == auto { 2.5 / 21 * calc.min(w, if h == auto { w } else { h }) } else if type(v) == length { v.to-absolute() } else if type(v) == ratio { v * full } else { v.length.to-absolute() + v.ratio * full }
  redline-meta((heading: c.func() == heading, top: here().position().y <= top + 1pt), label("redline-first-" + str(id)))
}

// Where mark `id` starts, given the location `at` of its start tag: on the
// later page where its first part is (see `redline-first`).
#let redline-start-at(at, id) = {
  let first = if id != none { query(label("redline-first-" + str(id))).at(0, default: none) }
  if first == none or first.location().page() <= at.page() { return at }
  let v = redline-value(first)
  let h = if v.heading { query(selector(heading).after(at)).at(0, default: none) }
  if v.top { first.location() } else if h != none and h.location().page() > at.page() { h.location() } else { at }
}

// Whether content starts with something laid out in a line of text.
#let redline-inline(c) = type(c) == content and (
  repr(c.func()) in ("text", "space", "strong", "emph", "box", "h", "link", "highlight", "underline", "strike", "overline", "super", "sub", "smallcaps", "smartquote", "ref", "cite", "footnote", "linebreak", "symbol")
    or c.func() in (math.equation, raw) and not c.block
    or c.func() == [].func() and c.children.len() > 0 and redline-inline(c.children.first())
    or repr(c.func()) == "styled" and redline-inline(c.child)
)

// The parts of a mark with its tags and frames: `style` applied to each run
// of parts that are not framed, `frame-style` to each framed part that shows
// text. A drawing, an image, or content with colours of its own is drawn as
// written, with only `plain` applied: its frame is the mark. A colour would
// reach the labels of a drawing but not its lines. Deleted content that is
// not struck, such content or a displayed equation, is crossed or tinted
// (see `redline-frame`). A box in the text that shows no text is a drawing
// too, framed at its own size. A figure's caption is styled all the same.
// The tags around content that takes no room are placed, so that they do
// not form a line.
#let redline-mark(kind, body, block, style, stroke, frame-style: x => x, plain: x => x, whole: false, step: none, id: none) = {
  let (lead, ch, trail) = redline-parts(body)
  let n = ch.len()
  let framed = ch.map(c => redline-framed(c, block and n == 1))
  let roomless = block and ch.all(redline-roomless)
  let styled = n > 0 and not roomless and not framed.last() and repr(ch.last().func()) == "styled"
  // A tag between blocks is at the top of the next block, and the bar starts
  // a little above it: a gap can be as narrow as the leading of a tight list.
  let start-dy = if roomless { -1em } else { -0.3em }
  let out = ()
  let run = ()
  for (i, c) in ch.enumerate() {
    if framed.at(i) {
      if run.len() > 0 { out.push(style(run.join())) }
      run = ()
      let start = if i == 0 { kind }
      let b = if c.func() == figure { c.body } else { c }
      let picture = not redline-textual(b)
      let own = picture or redline-own-colours(b)
      // A strike reaches only the words of an equation.
      let eq = kind == "del" and c.func() == math.equation
      let f = redline-frame(c, stroke, cross: kind == "del" and picture, tint: kind == "del" and (own or eq), whole: whole, start: start, end: if i == n - 1 { "end" }, step: step, id: id)
      out.push(if eq { plain(text(fill: stroke.paint, f)) } else if not own { frame-style(f) } else if c.func() == figure {
        show figure.caption: frame-style
        plain(f)
      } else { plain(f) })
    } else {
      // A tag in a box, joined to the text after it, starts the line the
      // text starts, on its baseline; a tag alone stays on the line before a
      // break. Before a block, a box would add a line.
      let between = block and not redline-inline(c)
      if i == 0 { run.push(if between { redline-tag(kind, start-dy, step: step, id: id) } else [#box(redline-tag(kind, -0.8em, step: step, id: id))\u{2060}]) }
      let c = if c.func() == box and c.at("body", default: none) != none and not redline-textual(c) {
        box(stroke: stroke, outset: redline-style.outset.top, {
          c
          if kind == "del" { context { let s = measure(c); redline-cross(stroke, s.width, s.height) } }
        })
      } else { c }
      run.push(if i == n - 1 and styled { redline-end-in(c) } else { c })
      if i == 0 and between and n > 1 and not roomless and id != none { run.push(redline-first(c, id)) }
    }
  }
  if run.len() > 0 { out.push(style(run.join())) }
  lead
  if n == 0 { redline-tag(kind, start-dy, step: step, id: id) }
  out.join()
  // Content computed at layout can set the page, and anything visible after
  // it would get a page of its own; such a tag has no width of its container.
  if n == 1 and block and not framed.last() and repr(ch.last().func()) in ("context", "layout") {
    context redline-tag("end", 0.3em, width: 0pt)
  } else if roomless or block and ch.last().func() in (list.item, enum.item, terms.item) {
    // After a list, a tag in the text would start a line of its own.
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
  let stroke = (bottom: 0.5pt + st.ins)
  // A box keeps a fill behind math but stops it from breaking across lines.
  // It reaches as far up and down as the highlight of the text around it,
  // from the ascender to the descender of the text's font, which inside
  // math is the math font.
  context {
    let edge(top, bottom) = measure(text(top-edge: top, bottom-edge: bottom)[x]).height
    let (ascender, descender) = (edge("ascender", "baseline"), edge("baseline", "descender"))
    show math.equation.where(block: false): it => context if measure(it).width < 0.2 * page.width {
      let h = measure(box(it)).height
      let depth = measure(box[#it#text(top-edge: 100pt, bottom-edge: "baseline", sym.zwj)]).height - 100pt
      let outset = (top: calc.max(0pt, ascender - h + depth), bottom: calc.max(0pt, descender - depth))
      box(fill: st.ins-fill, stroke: stroke, outset: outset, it)
    } else { it }
    highlight(fill: st.ins-fill, stroke: stroke, body)
  }
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
// Whether the `n`-th deleted reference, of `key`, can be laid out as a
// citation without changing the new document's bibliography or numbering:
// the new document cites the key, and cites no other key for the first time
// between here and its first citation of the key, other than keys cited
// before here, by the new document or by deleted citations laid out. Each
// deleted reference leaves a marker whatever it shows, so that this is
// decided from the order of markers and new citations alone and does not
// depend on what earlier deleted references showed.
#let redline-cite-safe(key, n) = {
  let items = query(selector(cite).or(<redline-deleted-ref>)).filter(x => not (x.has("label") and x.label == <redline-deleted-cite>))
  let seen = ()
  let m = 0
  for (j, x) in items.enumerate() {
    if x.func() == cite { seen.push(str(x.key)); continue }
    let k = redline-value(x)
    let rest = items.slice(j + 1).filter(y => y.func() == cite)
    let first = rest.position(y => str(y.key) == k)
    let safe = first != none and rest.slice(0, first).all(y => str(y.key) in seen)
    if m == n { return safe }
    if safe { seen.push(k) }
    m += 1
  }
  false
}

// Content `c` rebuilt with `map` applied to its body and children; `keep`
// keeps its label and the styles of a styled element.
#let redline-rebuild(c, map, keep: true) = {
  let f = c.func()
  let fs = c.fields()
  let label = fs.remove("label", default: none)
  if f == [].func() { return fs.children.map(map).join() }
  if repr(f) == "styled" { return if keep { f(map(fs.child), fs.styles) } else { map(fs.child) } }
  let keys = ("dest", "alignment", "angle", "count", "value").filter(k => k in fs)
  let positional = keys.map(k => fs.at(k))
  for k in keys { let _ = fs.remove(k) }
  let body = if "body" in fs { (map(fs.remove("body")),) } else { () }
  let children = fs.remove("children", default: ()).map(map)
  let out = f(..fs, ..positional, ..body, ..children)
  if keep and label != none [#out#label] else { out }
}

#let redline-neutral(c) = {
  if not redline-effect(c) { return c }
  let f = c.func()
  if f == metadata or repr(f) == "state-update" { return [] }
  if f == footnote { return [ (#redline-neutral(c.body))] }
  if f == ref {
    let sup = c.at("supplement", default: none)
    // Citations cannot be queried before Typst 0.14.
    return context {
      let n = query(selector(<redline-deleted-ref>).before(here())).len()
      redline-meta(str(c.target), <redline-deleted-ref>)
      if query(c.target).len() > 0 { c } else if sys.version >= version(0, 14, 0) and redline-cite-safe(str(c.target), n) {
        [#cite(c.target, supplement: if sup == auto { none } else { sup })<redline-deleted-cite>]
      } else {
        emph(str(c.target))
      }
    }
  }
  redline-rebuild(c, redline-neutral, keep: false)
}

// Content with `style` applied to the body of each footnote in it, which
// the page foot shows apart from the marks around the footnote. A show rule
// would make a second footnote.
#let redline-footnotes(c, style) = {
  let has(c) = type(c) == content and (c.func() == footnote or c.fields().values().any(v => has(v) or type(v) == array and v.any(has)))
  if not has(c) { return c }
  let map = x => redline-footnotes(x, style)
  let body = x => if type(x) == content { style(map(x)) } else { x }
  redline-rebuild(c, if c.func() == footnote { body } else { map })
}

// Deleted figures and headings stay out of outlines and bookmarks.
#let redline-del-unlisted(body) = {
  set figure(outlined: false)
  set heading(outlined: false, bookmarked: false)
  body
}

#let redline-counter-update = counter("").update(0).func()
#let redline-state-update = state("").update(none).func()

// The counters that deleted content can change: those of a name or label
// that the document updates by hand, and those of headings, equations and
// each kind of figure. Typst updates counters of its own elements too, such
// as that of numbered lines, which cannot be read at a location. Calls of a
// function are cached, so this is one query per layout pass for all
// deletions.
#let redline-counter-keys() = {
  let keys = query(redline-counter-update).map(u => u.key).filter(k => type(k) in (str, label))
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
#let redline-restore(start) = {
  // Counters are read at an element of the previous layout pass: there,
  // `here()` can be one that pass did not lay out, which reads as the end
  // of the document.
  let end = label(str(start) + "-end")
  redline-meta(none, end)
  context {
    let ends = query(selector(end).before(here()))
    if ends.len() == 0 { return }
    let end = ends.last().location()
    // Typst 0.11 orders the elements of right-to-left text by position, so
    // that the start can come after the end; nothing is restored then.
    let starts = query(selector(start).before(end))
    if starts.len() == 0 { return }
    let start = starts.last().location()
    let inside(s) = query(selector(s).after(start).before(end))
    // By the change inside the content rather than to the value before it:
    // that value depends on the updates of every earlier deletion, so that
    // setting it would take a layout pass per deletion to settle. The update
    // holds only the change, so that it is the same in every pass.
    for k in if redline-keep-states { () } else { redline-counter-keys() } {
      let (b, a) = (counter(k).at(start), counter(k).at(end))
      let d = b.enumerate().map(((i, v)) => a.at(i, default: 0) - v)
      if d.any(x => x != 0) or a.len() != b.len() {
        counter(k).update((..now) => d.enumerate().map(((i, x)) => calc.max(0, now.pos().at(i, default: 0) - x)))
      }
    }
    // Counting the state updates before each end, the tool's own apart, is
    // cheaper than finding those inside every deletion.
    let count(c) = c.at(end).first() - c.at(start).first()
    let changed = if count(counter(redline-state-update)) != count(counter(redline-state-update.where(key: "redline"))) {
      inside(redline-state-update).filter(u => u.key != "redline").map(u => u.key).dedup()
    } else { () }
    let lost = changed + if inside(redline-counter-update.where(key: page)).len() > 0 { (page,) } else { () }
    if lost.len() > 0 { redline-meta(lost, <redline-unrestored>) }
  }
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
// as a change from the old version to the new one, an identity, so that
// the binary can pair the pages of the two sides, its id and its content.
#let redline-hash(seed, body) = {
  let h = seed
  for b in array(bytes(repr(body))) { h = calc.rem(h * 31 + b, 1000000007) }
  h
}

#let redline-side-mark(kind, body, id: none, block: false) = {
  // A move ("move") is marked at its start, with its id, and at its end; its
  // `body` is its key.
  let kind = if redline-side == "old" { (ins: "del", del: "ins", move: "move").at(kind) } else { kind }
  let identity = if kind == "move" { body } else { redline-hash(if kind == "ins" { 1 } else { 2 }, body) }
  redline-meta((kind, identity, id, body, block), <redline-change>)
}

// A move is bracketed by tags of its own, because it encloses ordinary
// marks: "to" at the start of its new place and "end" at its end, which
// records its container as redline-tag does; "from" at the line at its old
// place. Start tags are found by their label. A tag next to a block-level
// element is not boxed, which would add a line; it then sits at the edge of
// the block instead of on a line of text. The "to" tag records whether the
// new place starts with a heading, the end tag also the text size.
#let redline-move-tag(kind, id, step: none, block: false, heading: false) = {
  let v = (kind: kind, id: id, step: step, block: block, heading: heading)
  if kind != "end" {
    let tag = redline-meta(v, label("redline-move-" + kind + "-" + str(id)))
    if block { tag } else { box(tag) }
  } else {
    let tag = layout(size => redline-meta(v + (width: size.width, gutter: columns.gutter, columns: page.columns, size: text.size), <redline-move>))
    if block { place(tag) } else [\u{2060}#box(tag)]
  }
}

#let redline-move-find(kind, id) = query(label("redline-move-" + kind + "-" + str(id))).at(0, default: none)

// Where the new place of move `id` starts: at its "to" tag, or at its
// heading if that is on a later page, as when a show rule starts a page
// with each heading; the tag before the heading is then on the page before.
#let redline-move-start(id) = {
  let to = redline-move-find("to", id)
  if to == none { return none }
  let at = to.location()
  if redline-value(to).heading {
    let h = query(selector(heading).after(at)).at(0, default: none)
    if h != none and h.location().page() > at.page() { return h.location() }
  }
  at
}

// The counting symbols of page numbering patterns that Typst 0.11 knows.
#let redline-counting = ("1", "a", "A", "i", "I", "α", "Α", "*", "一", "壹", "あ", "い", "ア", "イ", "א", "가", "ㄱ")

// The number printed on the page of `loc`, in the style of the first
// counting symbol of its numbering; none if the page has no number.
#let redline-printed(loc) = {
  let pattern = loc.page-numbering()
  if pattern == none { return none }
  let n = counter(page).at(loc).first()
  let symbol = if type(pattern) == str { pattern.clusters().find(c => c in redline-counting) }
  if symbol == none { str(n) } else { numbering(symbol, n) }
}

// The pages of the locations `locs` as a reader finds them, consecutive
// pages joined: by the number printed on them, with the page of this PDF
// where that differs, such as "p. 7 (PDF page 8)"; a page without a number
// by its page of this PDF, called so if other pages have numbers. Without
// `prefix`, "p." and "pp." are left out.
#let redline-page-names(locs, prefix: true) = {
  let numbered = query(<redline-page>).any(m => m.location().page-numbering() != none)
  let firsts = (:)
  for l in locs {
    if str(l.page()) not in firsts { firsts.insert(str(l.page()), l) }
  }
  let runs = ()
  for l in firsts.values().sorted(key: l => l.page()) {
    if runs.len() > 0 and runs.last().last().page() + 1 == l.page() { runs.last().push(l) } else { runs.push((l,)) }
  }
  runs.map(run => {
    let (a, b) = (run.first(), run.last())
    let one = run.len() == 1
    let (pa, pb) = (redline-printed(a), redline-printed(b))
    let span(x, y) = if one [#x] else [#x–#y]
    let pdf = [PDF #if one [page] else [pages] #span(a.page(), b.page())]
    let p = if prefix { if one [p. ] else [pp. ] }
    if pa == none or pb == none {
      if numbered or pa != pb { pdf } else [#p#span(a.page(), b.page())]
    } else if pa == str(a.page()) and pb == str(b.page()) [#p#span(pa, pb)] else [#p#span(pa, pb) (#pdf)]
  }).join[, ]
}

// Page numbers, or numbers of changes, in increasing order as runs, such as
// "2–4, 7".
#let redline-pages(ps) = {
  let runs = ()
  for p in ps {
    if runs.len() > 0 and runs.last().last() + 1 == p { runs.last().push(p) } else { runs.push((p,)) }
  }
  // `join` takes no default before Typst 0.12.
  if runs.len() == 0 { "–" } else { runs.map(r => str(r.first()) + if r.len() > 1 { "–" + str(r.last()) } else { "" }).join(", ") }
}

// Numbers of changes as the margin and the summary show them.
#let redline-number(ns) = text(fill: redline-style.number, number-type: "lining", redline-pages(ns.sorted().dedup()))

// Content as pieces that can be cut between: words of text, other elements
// whole.
#let redline-pieces(c) = if type(c) != content { (c,) } else if c.func() == [].func() {
  c.children.map(redline-pieces).flatten()
} else if c.func() == text and c.has("text") {
  c.text.split(" ").map(text).intersperse([ ])
} else { (c,) }

// The line at the old place of moved text, linked to its new place: the
// first words of what moved in quotes, as many as fit on one line. It
// takes the place of a paragraph, with the paragraph spacing above and below. Its arrow has
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
      let to = redline-move-start(m.id)
      let head = if to == none [Moved:] else {
        let down = m.at("down", default: true)
        let where = if to.page() != here().page() [to #redline-page-names((to,))] else if down [below] else [above]
        [#text(fill: redline-step-style(step).del, if down { sym.arrow.b } else { sym.arrow.t }) Moved #where:]
      }
      let line(k) = {
        let more = if k < pieces.len() or m.at("more", default: false) [ …]
        [#head #if k > 0 ["#style(pieces.slice(0, k).join())#more"] else { more }]
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
      if to == none { line } else { link(to, line) }
      redline-restore(label("redline-move-from-" + str(m.id)))
    }))
  }
  parbreak()
}

#let redline-ins(body, block: false, step: none, move: none, id: none) = {
  if move != none {
    let end = move.at("end", default: false)
    // Both ends, so that the pages a long move spans are shown.
    let side = if redline-side != none { redline-side-mark("move", move.key, id: if not end { move.id }) }
    return side + redline-move-tag(if end { "end" } else { "to" }, move.id, step: step, block: move.block, heading: move.at("heading", default: false))
  }
  let st = redline-step-style(step)
  if redline-side != none { redline-side-mark("ins", body, id: id) }
  if redline-side == "old" {
    let look = redline-del-look.with(st: st)
    redline-mark("del", redline-footnotes(body, look), block, look, st.del-frame, frame-style: look, step: step, id: id)
  } else {
    let body = redline-footnotes(body, b => redline-ins-rules(b, false, st: st))
    redline-mark("ins", body, block, b => redline-ins-rules(b, block, st: st), st.ins-frame, frame-style: text.with(fill: st.ins), step: step, id: id)
  }
}

// A deleted list or enum item, `item` as in redline-del, spaced as its list
// spaces items and with its marker in `fill` where the list puts markers.
#let redline-item(item, fill, body) = context {
  let numbered = item.number != none
  let start = if item.at("counted", default: false) and enum.start != auto { enum.start - 1 } else { 0 }
  let marker = if numbered { numbering(enum.numbering, start + item.number) } else {
    let m = list.marker
    if type(m) == array { m.at(calc.rem(item.depth - 1, m.len())) } else if type(m) == function { m(item.depth - 1) } else { m }
  }
  let (indent, gap, spacing) = if numbered { (enum.indent, enum.body-indent, enum.spacing) } else { (list.indent, list.body-indent, list.spacing) }
  let spacing = if spacing != auto { spacing } else if item.tight { par.leading } else if sys.version >= version(0, 12, 0) { par.spacing } else { 1.2em }
  let w = measure(marker).width
  set par(first-line-indent: 0pt)
  block(above: spacing, below: spacing, inset: (left: if item.inside { 0pt } else { indent + w + gap }), {
    h(-w - gap)
    text(fill: fill, marker)
    h(gap)
    body
  })
}

// On a side of the side-by-side layout, a deletion is content the version
// does not have: the page next to it shows it.
#let redline-del(body, block: false, heading: none, item: none, gap: false, step: none, move: none, id: none) = {
  let st = redline-step-style(step)
  // The binary pairs the pages of a move's two places by the marks around
  // its content, which each side has.
  if redline-side != none and move != none { return }
  if redline-side != none { return redline-side-mark("del", body, id: id, block: block or heading != none or item != none) }
  if move != none { return redline-move-line(move, body, step) }
  let block = block or heading != none or item != none
  // Content that draws nothing stays in the paragraph it was in, and the
  // summary lists its code.
  let unseen = redline-unseen(body)
  let apart = block and not unseen
  let body = redline-neutral(body)
  // Deleted white space alone, such as a non-breaking space, is shown by a
  // space sign: struck, it would read as a hyphen.
  if type(body) == content and body.func() != raw and body.has("text") and body.text.trim() == "" {
    body = box(width: 0.45em, height: 0.25em, stroke: (x: 0.07em + st.del, bottom: 0.07em + st.del))
  }
  if apart { parbreak() }
  let rules = redline-del-rules.with(st: st)
  // A deleted line break inside a paragraph is shown by a sign, as breaking
  // the line would move the text after it. The sign can come from a font
  // with a taller line; its edges keep it from raising the line.
  let look = if heading != none {
    b => rules(text(weight: "bold", size: if heading == 1 { 1.4em } else { 1.2em }, b))
  } else if block { rules } else {
    b => { show linebreak: text(top-edge: "x-height", bottom-edge: "baseline", "\u{21B5}"); rules(b) }
  }
  // A label of its own, which is found faster than the last of many.
  let start = label("redline-del-start-" + repr(id))
  redline-meta(none, start)
  if unseen { redline-ints((id,), <redline-unseen>) }
  // Its tags take no room either: a box or a placed element in a paragraph
  // would add a space or break it.
  let mark = if unseen [#redline-tag("del", -0.8em, step: step, id: id)#body#context redline-tag("end", 0.3em, width: 0pt)] else {
    redline-mark("del", body, block, look, st.del-frame, frame-style: look, plain: redline-del-unlisted, whole: true, step: step, id: id)
  }
  if item == none { mark } else { redline-item(item, st.del, mark) }
  redline-restore(start)
  if apart { parbreak() } else if gap and not unseen { h(0.25em) }
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

// A line at the top edge of the first page, above any header, that links
// to the summary and says how many changes it lists that are not marked,
// or that there are none.
#let redline-pointer(x, margin, width) = {
  let at = query(<redline-summary>)
  if at.len() == 0 { return }
  let v = redline-value(at.first())
  let n = v.unmarked
  let more = if n > 0 [ · #n #if n == 1 [change is] else [changes are] listed there, not marked] else if v.notes or query(<redline-unrestored>).len() > 0 [ · with notes on what this PDF shows differently]
  set text(size: 6.5pt, lang: "en", dir: ltr, weight: "regular", style: "normal")
  set par(justify: false)
  let lead = if v.at("empty", default: false) [typst-redline found no changes] else [Changes marked by typst-redline]
  let pages = redline-page-names((at.first().location(),))
  // The colours inside the link, which the document can style. Without
  // its first words where it would wrap; on white and at the right, to read
  // on a header band beside a title.
  let line(head) = [#text(fill: luma(110), head)#text(fill: redline-style.del, more)]
  let full = line[#lead · summary on #pages]
  let line = context if measure(full).width <= width { full } else { line[Summary on #pages] }
  let line = box(fill: white, outset: (x: 2pt, y: 1.5pt), radius: 1pt, line)
  place(top + left, dx: x, dy: calc.max(2pt, margin / 4 - 3pt), block(width: width, align(end, link(at.first().location(), line))))
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
  let tags = query(<redline-tag>)
  let n = calc.quo(tags.len(), 2)
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
  if pg == 1 and redline-side == none { redline-pointer(left, top, body) }
  if n == 0 and query(<redline-move>).len() == 0 { return }
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
  // The distance of the bars from the text in a margin `m` wide, smaller in
  // a narrow margin, so that a move's bracket three lanes out and a number
  // beyond it stay clear of the page edge.
  let gap(m) = calc.max(3pt, calc.min(st.bar-gap, m / 2, m - 3 * st.lane - 13pt))
  let bar-x(s, e, cw, g, n, lane) = {
    let (k, cols, cw, g) = column(s, e, cw, g, n)
    let at = calc.min(s.x, e.x) - left
    if cols > 1 and k > 0 {
      let l = calc.max(0pt, calc.min(st.lane, (g - st.bar-width) / 2))
      let edge = left + k * (cw + g) - g
      (calc.max(left + k * (cw + g) - calc.min(st.bar-gap, g / 2) + l / 2 - lane * l, edge + 3pt), false, edge)
    } else if cols == 1 and at > cw and at + cw > body - 1pt {
      (calc.min(left + body + gap(right) + lane * st.lane, w - st.bar-width / 2), true, w)
    } else {
      (calc.max(left - gap(left) - lane * st.lane, st.bar-width / 2), false, 0pt)
    }
  }
  // Whether the number of a change goes in the right margin: the margin
  // next to its column outside the bars and brackets, or for a column
  // between others the nearer one.
  let number-at(s, e, cw, g, n) = {
    let (k, cols, ..) = column(s, e, cw, g, n)
    if cols > 1 { 2 * k >= cols - 1 } else { bar-x(s, e, cw, g, n, 0).at(1) }
  }
  let nums = query(<redline-numbers>)
  let nums = if nums.len() > 0 and redline-side != "old" { redline-value(nums.first()) }
  let number-size = calc.max(6pt, w / 100)
  let numbers = ()
  // The (right margin, top, bottom) of notes that run along a bracket.
  let along-notes = ()
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
    let (s, e) = (redline-start-at(a.location(), redline-value(a).at(6, default: none)).position(), b.location().position())
    if s.page > pg { break }
    let (a, b) = (redline-value(a), redline-value(b))
    // A mark from an earlier page that ends above the text area has nothing
    // left here; one in a header starts and ends above it.
    if e.page > pg or e.y > top or s.page == pg {
      // A mark in the text area starts no higher than just above it.
      let y0 = if s.page != pg { top } else if s.y < top { s.y + a.at(1).to-absolute() } else { calc.max(s.y + a.at(1).to-absolute(), top - 1em.to-absolute()) }
      let y1 = if e.page == pg { e.y + b.at(1).to-absolute() } else { bottom }
      let ins = a.at(0) == "ins"
      // Deletions are one lane further out.
      let (x, ..) = bar-x(s, e, b.at(2, default: none), b.at(3, default: none), b.at(5, default: none), if ins { 0 } else { 1 })
      let sst = redline-step-style(a.at(4, default: none))
      bars.push((x, y0 + 1pt, y1 - 1pt, ins, if ins { sst.ins } else { sst.del }))
      let id = a.at(6, default: none)
      let num = if nums != none and s.page == pg and id != none { nums.at(str(id), default: none) }
      let at = if num != none { number-at(s, e, b.at(2, default: none), b.at(3, default: none), b.at(5, default: none)) }
      // A number stands on the baseline of the line a mark starts on, or at
      // the top of its bar for a block.
      let y = if a.at(1) == -0.8em { s.y - 0.7 * number-size } else { y0 + 1pt }
      if at != none { numbers.push((at, y, num)) }
    }
    i += 1
  }
  // Bars of one kind and colour in one place that overlap, or are too close
  // for a line of text between them, are drawn as one, so that changes on
  // consecutive lines share a bar. Between paragraphs, a gap can be as
  // wide as an unchanged line, so bars there stay apart.
  let join = 0.5em.to-absolute()
  let merged = ()
  for bar in bars.sorted(key: b => (b.at(0).pt(), b.at(1).pt())) {
    let last = if merged.len() > 0 { merged.last() }
    if last != none and last.at(0) == bar.at(0) and last.slice(3) == bar.slice(3) and bar.at(1) <= last.at(2) + join {
      merged.last().at(2) = calc.max(last.at(2), bar.at(2))
    } else { merged.push(bar) }
  }
  // A deletion bar is thinner and dashed, with whole dashes at its ends and
  // at least three, so that a short one does not read as a colon.
  for (x, y0, y1, ins, paint) in merged {
    let len = calc.max(y1 - y0, 0pt)
    let k = calc.max(3, calc.round((len + 1.5pt) / 4.5pt))
    let space = calc.min(1.5pt, len / (2 * k))
    let stroke = if ins { st.bar-width + paint } else {
      (paint: paint, thickness: st.del-bar-width, dash: ((len - (k - 1) * space) / k, space))
    }
    place(redline-top-left, dx: x, dy: y0, line(angle: 90deg, length: len, stroke: stroke))
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
    } else { bar-x(s, e, cw, g, n, 3) }
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
    let (s, e) = (redline-move-start(v.id).position(), t.location().position())
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
    // The move's number stands at the top of its bracket, a note across the
    // margin below it.
    let num = if nums != none { nums.at("m" + str(v.id), default: none) }
    let at = if num != none { number-at(s, e-in, v.width, v.gutter, v.at("columns", default: none)) }
    if at != none { numbers.push((at, if redline-value(a).block { y0 } else { s.y - 0.7 * number-size }, num)) }
    let below = if at != none { 1.3 * number-size } else { 0pt }
    // Notes that run up along the bracket have no arrow, which would turn.
    let (notes, along) = if other != none and other.location().page() != pg {
      let n = redline-page-names((other.location(),))
      let arrow = if other.location().page() < pg { sym.arrow.t } else { sym.arrow.b }
      (([moved from #n #arrow], [moved#linebreak()from #n #arrow], [moved]), ([moved from #n], [moved]))
    } else { (([moved],), ([moved],)) }
    let free = if right-side { edge - 2pt - x - 3pt } else { x - 3pt - edge - 2pt }
    let sst = redline-step-style(redline-value(a).step)
    // The note grows with the text, as on slides. Where the margin is too
    // narrow for it, it runs up along the bracket, or else is small.
    let size = calc.min(calc.max(v.at("size", default: 0pt).to-absolute() * 6.5 / 11, 6.5pt), 14pt)
    let styled(notes, size) = notes.map(n => {
      set par(leading: 0.4em)
      text(size: size, fill: if redline-side == "old" { sst.del } else { sst.ins }, top-edge: "cap-height", bottom-edge: "baseline", hyphenate: false, n)
    })
    let link-to(note) = if other != none { link(other.location(), note) } else { note }
    let note = styled(notes, size).find(n => measure(n).width <= free)
    let turned = if note == none { styled(along, size).find(n => measure(n).height <= free and measure(n).width <= y1 - y0) }
    if note == none and turned == none { note = styled(notes, 6.5pt).find(n => measure(n).width <= free) }
    if turned != none {
      let (w, h) = (measure(turned).width, measure(turned).height)
      place(redline-top-left, dx: if right-side { x + 3pt } else { x - 3pt - h }, dy: y0 + w, rotate(-90deg, origin: redline-top-left, link-to(turned)))
      along-notes.push((right-side, y0, y0 + w))
    } else if note != none {
      place(redline-top-left, dx: if right-side { x + 3pt } else { edge + 2pt }, dy: y0 + below, box(width: free, align(if right-side { start } else { end }, link-to(note))))
    }
  }
  // The old place of a move has its number too.
  let moves = if nums != none { nums.pairs().filter(((k, _)) => k.starts-with("m")) } else { () }
  for (k, num) in moves {
    let from = redline-move-find("from", k.slice(1))
    if from == none or from.location().page() != pg { continue }
    let p = from.location().position()
    numbers.push((number-at(p, p, none, none, none), p.y - 0.7 * number-size, num))
  }
  // On the new side of the side-by-side layout, a deletion has its number
  // where it was: a deleted block's mark is at the end of the block before
  // it, on its baseline, and the number goes below that.
  if nums != none and redline-side == "new" {
    for c in query(<redline-change>).filter(c => c.location().page() == pg) {
      let (kind, _, id, _, block) = redline-value(c)
      let num = if kind == "del" { nums.at(str(id), default: none) }
      let p = c.location().position()
      let at = if num != none { number-at(p, p, none, none, none) }
      if at != none { numbers.push((at, p.y + if block { 0.3 } else { -0.7 } * number-size, num)) }
    }
  }
  // A number where a note runs along a bracket stands above the bracket.
  let numbers = numbers.map(((r, y, num)) => {
    let along = along-notes.find(((side, top, bottom)) => side == r and y > top - number-size and y < bottom)
    (r: r, y: if along == none { y } else { along.at(1) - 1.3 * number-size }, ns: (num,))
  })
  // A number is shown once on a page, where its change starts, but at both
  // places of a move; numbers on about one line on one side are shown
  // together.
  let (shown, seen) = ((), ())
  for n in numbers.sorted(key: n => n.y.pt()) {
    if n.ns.first() in seen { continue }
    if moves.all(((_, m)) => m not in n.ns) { seen += n.ns }
    shown.push(n)
  }
  let together(numbers) = {
    let lines = ()
    for n in numbers.sorted(key: n => n.y.pt()) {
      let i = lines.position(l => l.r == n.r and n.y - l.y < 1.3 * number-size)
      if i == none { lines.push(n) } else { lines.at(i).ns += n.ns }
    }
    lines
  }
  // Numbers stand outside the bars and brackets, ending there in the left
  // margin and starting there in the right one. Where they do not fit, they
  // stand in the other margin if they fit there, or else as large as fits,
  // down to two thirds of their size, at the edge of the page.
  let anchor(r) = if r { calc.min(left + body + gap(right) + 3 * st.lane, w - st.bar-width / 2) + 3pt } else { calc.max(left - gap(left) - 3 * st.lane, st.bar-width / 2) - 3pt }
  let room(r) = if r { w - 1pt - anchor(r) } else { anchor(r) - 1pt }
  let label(ns, size) = text(size: size, top-edge: "cap-height", bottom-edge: "baseline", redline-number(ns))
  let width(ns) = measure(label(ns, number-size)).width
  let lines = together(together(shown).map(l => {
    if width(l.ns) > room(l.r) and width(l.ns) <= room(not l.r) { l.r = not l.r }
    l
  }))
  for (r, y, ns) in lines {
    let tw = width(ns)
    let size = number-size * calc.max(2 / 3, calc.min(1, room(r) / tw))
    let t = label(ns, size)
    let tw = measure(t).width
    place(redline-top-left, dx: if r { calc.min(anchor(r), w - tw - 1pt) } else { calc.max(anchor(r) - tw, 1pt) }, dy: calc.max(y, 1pt), t)
  }
  // The legend goes at the bottom edge of the page, as the pointer line goes
  // at the top edge, if it stays clear of the footer. The footer lies in the
  // bottom margin below its descent, aligned in what is left; a page number
  // is in it unless it is at the top, and is taken to be one line high. In
  // Typst 0.11 an unset footer reads as none.
  let hist = query(<redline-history>)
  let legend = if merged.len() > 0 and hist.len() > 0 { redline-legend(redline-value(hist.first()), body) }
  if legend != none {
    let margin = side(("bottom",), h)
    let foot = if page.footer not in (none, auto) { page.footer } else if page.numbering != none and page.number-align.y != redline-top-left.y [0]
    let foot-end = if foot == none { 0pt } else {
      let descent = absolute(page.footer-descent, margin)
      let fh = measure(block(width: body, foot)).height
      let y = if type(foot) == content and foot.func() == align { foot.alignment.y } else { none }
      let free = margin - descent - fh
      descent + fh + if y == redline-bottom-left.y { free } else if y == horizon { free / 2 } else { 0pt }
    }
    let edge = calc.max(2pt, margin / 4 - 3pt)
    if margin - edge - measure(legend).height >= foot-end + 3pt {
      place(redline-bottom-left, dx: left, dy: -edge, legend)
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
  show hide: it => if it.has("label") and it.label == <redline-tag-at> { it } else {
    show <redline-tag-at>: none
    it
  }
  body
}

// `body` with the number `n` of its change, if not none, in the indent
// before it.
#let redline-numbered(n, body) = if n == none { body } else {
  block({
    place(dx: -4em, box(width: 3.4em, align(right, redline-number((n,)))))
    body
  })
}

// What code is, by its first words: a label, a definition or a set or show
// rule, or none.
#let redline-code-kind(code) = {
  let t = code.trim()
  let mono = text.with(font: "DejaVu Sans Mono", size: 0.9em)
  let m = t.match(regex("^#?(let|set|show)\s+([^\s(=:]+)"))
  if t.match(regex("^<[\w:.-]+>$")) != none [label #mono(t)] else if m == none { none } else if m.captures.first() == "let" [definition of #mono(m.captures.last())] else [#mono(m.captures.join(" ")) rule]
}

// Changed code that is not marked, (old, new, what): what changed in words,
// then the old code in the colour of deletions and the new in that of
// insertions, or the code once in grey if the words say all of it. Without
// words from `what`, a label, definition or rule is named. Text, not raw, so
// that the document's show rules on raw leave it alone. With `n`, its number
// stands in the indent before it.
#let redline-code(c, n: none) = redline-numbered(n, {
  let st = redline-style
  let mono = text.with(font: "DejaVu Sans Mono", hyphenate: false)
  let value(v, fill, f) = if "text" in v {
    ["] + v.text.map(((t, changed)) => if changed { text(fill: fill, f(t)) } else { t }).join() + ["]
  } else { mono(size: 0.9em, fill: fill, v.code) }
  let words = c.at("what", default: ()).map(w => if "file" in w [#mono(size: 0.9em, w.file) changed] else if "note" in w { w.note } else {
    let name = mono(size: 0.9em, w.name)
    if w.old == none [#name: #value(w.new, st.ins, x => x) added] else if w.new == none [#name: #value(w.old, st.del, x => x) removed] else [#name: #value(w.old, st.del, strike) → #value(w.new, st.ins, underline)]
  })
  let (old, new) = (redline-code-kind(c.old), redline-code-kind(c.new))
  let named = if words.len() > 0 { none } else if c.old == "" and new != none [#new added] else if c.new == "" and old != none [#old removed] else if old != none and old == new [#old changed]
  if named != none { words.push(named) }
  if words.len() > 0 { block(above: 0.6em, below: 0.4em, words.join[; ]) }
  // Values too long for a line are elided, and a change in what is called
  // has no argument to name.
  let what = c.at("what", default: ())
  let elided(v) = v != none and v.at("code", default: none) == "…"
  let args = what.filter(w => "name" in w)
  let complete = what.len() > 0 and not args.any(w => elided(w.old) or elided(w.new)) and (c.old == c.new or args.len() > 0)
  // Where the words say it all, the code that changed is shown once, grey,
  // to say where; the words do not name what is called.
  // A label is said in full by its name.
  let rows = if named != none and (c.old + c.new).trim().starts-with("<") { () } else if complete or c.old == c.new { ((if c.new == "" { c.old } else { c.new }, "", luma(110)),) } else { ((c.old, "−", st.del), (c.new, "+", st.ins)).filter(r => r.first() != "") }
  let paint(fill, t) = if fill == none { t } else { text(fill: fill, t) }
  set text(font: "DejaVu Sans Mono", size: 0.8em, hyphenate: false)
  set par(justify: false)
  if rows.len() == 0 { } else if rows.len() == 1 and rows.first().at(1) == "" { block(above: 0.4em, paint(rows.first().at(2), rows.first().first())) } else {
    grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, ..rows.map(((code, sign, fill)) => (paint(fill, sign), paint(fill, code))).flatten())
  }
})

// A bibliography entry: as the document prints it if `printed` and the
// document cites it and has a bibliography, else by its authors and year
// from `names`, else by its key. Citations cannot be queried before Typst
// 0.14, where the key is taken to be cited.
#let redline-bib-entry(key, names, printed) = context {
  let cited(c) = c.key == label(key) and not (c.has("label") and c.label == <redline-deleted-cite>)
  if printed and query(bibliography).len() > 0 and (sys.version < version(0, 14, 0) or query(cite).any(cited)) {
    cite(label(key), form: "full")
  } else { names.at(key, default: redline-mono(key)) }
}

// The bibliography entries and other files that changed, as (title, lines)
// pairs, numbered from `first` if it is not none.
#let redline-bib(bib, first: none) = {
  let names = bib.at("names", default: (:))
  let entries = (([New], bib.added, true), ([No longer cited], bib.removed, false), ([Changed], bib.changed, true))
  let lines = entries.map(((what, keys, printed)) => keys.map(k => [#what: #redline-bib-entry(k, names, printed)])).flatten()
  let files = bib.at("files", default: ())
  let number(i) = if first != none { first + i }
  let lines = lines.enumerate().map(((i, l)) => redline-numbered(number(i), l))
  let files = if first == none { if files.len() > 0 { (files.map(redline-mono).join[, ],) } else { () } } else {
    files.enumerate().map(((i, f)) => redline-numbered(number(lines.len() + i), redline-mono(f)))
  }
  let out = ()
  if lines.len() > 0 { out.push(([Bibliography entries], lines)) }
  if files.len() > 0 { out.push(([Images and data files that changed], files)) }
  out
}

// Arguments that keep a block with the next one on its page, where Typst
// can.
#let redline-sticky = if sys.version >= version(0, 12, 0) { (sticky: true) } else { (:) }

#let redline-head(body) = block(above: 1.5em, below: 0.75em, ..redline-sticky, text(weight: "bold", body))

// The changes that are not marked: `groups` is an array of (title, code:
// array of (old, new, id)), `bib` as for `redline-summary`, `nums` the
// numbers of changes from `redline-numbering`, or none to show none.
#let redline-not-marked(groups, bib, nums: none) = {
  let bib = redline-bib(bib, first: if nums != none { calc.max(0, ..nums.values()) + 1 })
  if groups.len() + bib.len() == 0 { return }
  let group(title, body) = block(below: 0.9em, { block(below: 0.55em, ..redline-sticky, emph(title)); pad(left: 1.2em, body) })
  redline-head[Not marked in place]
  if groups.len() > 0 {
    block(below: 0.9em, text(fill: luma(110))[Changes in the source that the pages do not mark, such as a setting.])
    for (title, code) in groups { group(title, code.map(c => redline-code(c, n: if nums != none { nums.at(str(c.id), default: none) })).join(v(0.3em))) }
  }
  for (title, lines) in bib { group(title, lines.join(if nums == none { linebreak() })) }
}

// Notes on what this PDF shows differently from the new version, or
// nothing without any.
#let redline-notes(notes) = if notes.len() > 0 {
  redline-head[What this PDF shows differently]
  block(stroke: (left: 1.5pt + redline-style.del), inset: (left: 0.8em, y: 0.2em), list(..notes))
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
  {
    redline-head[Steps from #old to #new]
    set par(justify: false)
    grid(columns: (auto, auto, auto, 1fr), column-gutter: 0.6em, row-gutter: 0.6em, ..rows)
  }
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
  {
    redline-head[Commits from #old to #new]
    if hist.earlier > 0 { block(emph[#hist.earlier earlier commits, not listed]) }
    grid(columns: if hist.steps { (auto, auto, auto, 1fr) } else { (auto, auto, 1fr) }, column-gutter: 0.6em, row-gutter: 0.6em, ..rows.flatten())
  }
}

#let redline-move-bracket = {
  let s = redline-style.move-stroke + redline-style.move
  box(width: 4pt, height: 0.65em, {
    place(dx: 0.5pt, line(angle: 90deg, length: 0.65em, stroke: s))
    place(dx: 0.5pt, line(length: 2.5pt, stroke: s))
    place(dx: 0.5pt, dy: 0.65em, line(length: 2.5pt, stroke: s))
  })
}

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
#let redline-lost(marks, shown) = marks.filter(m => if m.at("from", default: false) { false } else if "move" in m {
  query(label("redline-move-to-" + str(m.move))).len() == 0
} else { str(m.id) not in shown })

// Marks as the summary lists them: an insertion that continues a deletion
// is listed with it, old and new.
#let redline-joined(marks) = {
  let out = ()
  for m in marks {
    let i = out.len() - 1
    if m.at("joined", default: false) and m.old == "" and i >= 0 and out.at(i).new == "" and "move" not in out.at(i) {
      out.at(i).new = m.new
    } else { out.push(m) }
  }
  out
}

// The number of each change of `entries` (as for `redline-summary`), by the
// id of each of its marks and changes of code, and by "m" and the number of
// each move: changes are numbered in the order they are laid out in, a mark
// that continues the one before it shares its number, and a change that is
// not laid out comes after the change before it in the source.
#let redline-numbering(entries) = {
  let items = entries.map(e => e.marks + e.code).flatten().sorted(key: m => m.id)
  let key = (:)
  for m in items {
    let k = if "move" in m { "m" + str(m.move) } else if m.at("joined", default: false) and key.len() > 0 { key.values().last() } else { str(m.id) }
    key.insert(str(m.id), k)
  }
  let moves = items.filter(m => "move" in m).map(m => label("redline-move-to-" + str(m.move)))
  let rank = (:)
  for t in query(if moves.len() > 0 { selector(<redline-tag>).or(..moves) } else { selector(<redline-tag>) }) {
    let v = redline-value(t)
    let k = if type(v) == dictionary { "m" + str(v.id) } else if v.at(6) != none { key.at(str(v.at(6)), default: none) }
    if k != none and k not in rank { rank.insert(k, rank.len()) }
  }
  // The items are in source order, and sorting keeps the order of equals.
  let last = -1
  let order = ()
  for m in items {
    let k = key.at(str(m.id))
    if k in rank { last = rank.at(k) }
    order.push((2 * last + if k in rank { 0 } else { 1 }, k))
  }
  let number = (:)
  for (_, k) in order.sorted(key: o => o.first()) {
    if k not in number { number.insert(k, number.len() + 1) }
  }
  let nums = (:)
  for (id, k) in key { nums.insert(id, number.at(k)) }
  for (k, n) in number { if k.starts-with("m") { nums.insert(k, n) } }
  nums
}

// The number and title of the heading `el`, as the summary lists it.
#let redline-heading-name(el, title) = {
  let num = if el.numbering != none { numbering(el.numbering, ..counter(heading).at(el.location())) + h(0.5em) }
  // A word joiner keeps the number on the line of the title's first word.
  if num == none { title } else [#box(num)#sym.wj#title]
}

// The text of content without its deletions, whose marks must not be laid
// out again.
#let redline-plain(c) = if type(c) == str { c } else if type(c) != content { "" } else if c.func() in (strike, footnote, metadata) { "" } else if c.has("text") and type(c.text) == str { c.text } else if c.func() == [ ].func() { " " } else if c.has("children") {
  // An empty sequence joins to none.
  ("", ..c.children.map(redline-plain)).join()
} else if c.has("body") { redline-plain(c.body) } else if c.has("child") { redline-plain(c.child) } else { "" }

// The marks among `marks` that are laid out, grouped by the last heading of
// level 1 or 2 before them, or none: array of (heading, count: changes other
// than moves, moves: numbers of the moves, at: locations, marks). Deleted
// headings, which are neither outlined nor bookmarked, do not count.
#let redline-under-headings(marks) = {
  let at = (:)
  for t in redline-starts() {
    let id = redline-value(t).at(6)
    if id != none and str(id) not in at { at.insert(str(id), redline-start-at(t.location(), id)) }
  }
  let kept(x) = x.level <= 2 and (x.at("outlined", default: true) or x.at("bookmarked", default: auto) != false)
  let groups = ()
  for m in marks {
    let from = m.at("from", default: false)
    let loc = if from { redline-move-find("from", m.move) } else if "move" in m { redline-move-start(m.move) } else { at.at(str(m.id), default: none) }
    if from and loc != none { loc = loc.location() }
    if loc == none { continue }
    let head = query(selector(heading).before(loc)).filter(kept).at(-1, default: none)
    let i = groups.position(g => if head == none { g.heading == none } else { g.heading != none and g.heading.location() == head.location() })
    if i == none {
      groups.push((heading: head, count: 0, moves: (), out: 0, at: (), marks: ()))
      i = groups.len() - 1
    }
    groups.at(i).at.push(loc)
    groups.at(i).marks.push(m)
    if from { groups.at(i).out += 1 } else if "move" in m { groups.at(i).moves.push(m.move) } else if not m.at("joined", default: false) { groups.at(i).count += 1 }
  }
  groups
}

// The rows of the summary's changes by section, each (level, name, heading:
// the location of its heading or none, what: its changes in words, ns: their
// numbers, pages: the pages they start on). `lost` are the marks of each
// entry that were not laid out, joined, and `nums` the numbers from
// `redline-numbering`. A heading that is not a section of the source, such
// as one written as a call or inside a call, has a row of its own for the
// changes laid out under it. Changes laid out under a section of the
// source, such as in a page footer, stay with the section that has their
// code.
#let redline-section-rows(info, lost, nums) = {
  let st = redline-style
  let numbers(ms) = ms.map(m => nums.at(str(m.id), default: none)).filter(n => n != none)
  let counted(l) = l.filter(m => not m.at("joined", default: false)).len()
  let what(n, moves, unmarked, out: 0, ..label) = {
    let parts = label.pos()
    if n > 0 { parts.push[#n~#if n == 1 [change] else [changes]] }
    if moves > 0 { parts.push[#moves~#if moves == 1 [move] else [moves]] }
    if out > 0 { parts.push[#out~moved away] }
    [#parts.join[, ]#if unmarked > 0 [ (#unmarked~not marked)]]
  }
  let row(level, name, what, ns, at, heading: none) = ((level: level, name: name, heading: heading, what: what, ns: ns, pages: at.map(l => l.page()).dedup().sorted()),)
  let heading-row(level, el, title, what, ns, at) = {
    let loc = el.location()
    row(level, redline-heading-name(el, title), what, ns, if at.len() > 0 { at } else { (loc,) }, heading: loc)
  }
  let sections = info.at("sections", default: ())
  let own(g) = g.heading == none or g.heading.at("label", default: none) in sections
  let rows = ()
  for (e, l) in info.entries.zip(lost) {
    let moves = e.at("moves", default: 0)
    // Passages that moved out, counted where they arrived.
    let out = e.marks.filter(m => m.at("from", default: false)).len()
    let k = e.code.len() + counted(l)
    let ns = numbers(e.marks + e.code)
    let groups = redline-under-headings(e.marks)
    if e.deleted {
      rows += row(e.level, text(fill: st.del, strike(e.title)), what(0, 0, k, [deleted section]), ns, groups.map(g => g.at).flatten())
      continue
    }
    // What is laid out before the section's own heading, such as its entry
    // in an outline and the outline's title, is not part of it.
    let el = if e.target != none { query(e.target).at(0, default: none) }
    let after(loc) = el == none or {
      let (a, b) = (loc.position(), el.location().position())
      (a.page, a.y.pt()) >= (b.page, b.y.pt())
    }
    let found = groups.filter(g => not own(g) and after(g.heading.location()))
    let placed = found.map(g => g.count).sum(default: 0)
    let left = calc.max(0, moves - found.map(g => g.moves).flatten().dedup().len())
    let elsewhere = found.map(g => numbers(g.marks)).flatten()
    let away = out - found.map(g => g.out).sum(default: 0)
    let label = if e.new { ([new section],) } else if e.at("moved", default: false) { ([moved section],) } else { () }
    if found.len() == 0 or e.count > placed or left > 0 or away > 0 or k > 0 or label.len() > 0 {
      let what = what(e.count - placed, left, k, out: away, ..label)
      let (ns, at) = (ns.filter(n => n not in elsewhere), groups.filter(g => g not in found).map(g => g.at).flatten().filter(after))
      // A heading that the document does not lay out, such as one in a
      // body that a template drops, is listed by its title alone.
      rows += if el == none { row(e.level, e.title, what, ns, at) } else { heading-row(e.level, el, e.title, what, ns, at) }
    }
    for g in found {
      rows += heading-row(g.heading.level, g.heading, redline-plain(g.heading.body).trim(), what(g.count, g.moves.dedup().len(), 0, out: g.out), numbers(g.marks), g.at)
    }
  }
  rows
}

// For the summary and the binary: the pages with changes but without bars,
// whichever pages they are. For the side-by-side layout also the number of
// pages before the summary and of the summary's pages, the ids of the marks
// that were not laid out, each change's kind (0
// deleted, 1 inserted, 3 the start or end of a move's place), identity, mark
// (its id plus one, or 0) and page, and the same for each heading (kind 2),
// whose identity is its text, all as one flat array of integers.
#let redline-layout() = {
  let drawn = query(<redline-page>).map(m => m.location().page())
  let unbarred = query(selector(<redline-tag>).or(<redline-move>)).map(t => t.location().page()).dedup().filter(p => p not in drawn)
  redline-ints(unbarred, <redline-unbarred>)
  if redline-side != none {
    let changes = query(<redline-change>).map(m => {
      let (kind, identity, id, ..) = redline-value(m)
      let start = if kind == "move" { if id != none { redline-move-start(id) } } else { redline-start-at(m.location(), id) }
      ((del: 0, ins: 1, move: 3).at(kind), identity, if id == none { 0 } else { id + 1 }, if start != none { start.page() } else { m.location().page() })
    })
    let headings = query(heading).map(h => (2, redline-hash(3, h.body), 0, h.location().page()))
    let triples = (..changes, ..headings).flatten()
    let lost = query(<redline-lost>).map(redline-value).flatten()
    let (start, end) = (query(<redline-summary>), query(<redline-summary-end>)).map(q => q.map(m => m.location().page()))
    let (pages, summary) = if start.len() > 0 and end.len() > 0 { (start.first() - 1, end.first() - start.first() + 1) } else { (drawn.len() + unbarred.len(), 0) }
    redline-ints((pages, summary, unbarred.len(), ..unbarred, lost.len(), ..lost, triples.len(), ..triples), <redline-layout>)
  }
}

// The legend of the marks; `info` as for `redline-summary`.
// Each kind of mark drawn as on the pages, with a few words.
#let redline-marks-legend(info) = {
  let steps = info.at("history", default: (steps: false)).steps
  let st = if steps { redline-style + redline-colours(luma(70)) } else { redline-style }
  let frame(stroke) = box(width: 1.4em, height: 0.75em, stroke: stroke)
  let bar(paint, width, dash) = box(height: 0.9em, baseline: 0.1em, line(angle: 90deg, length: 0.9em, stroke: (paint: paint, thickness: width, dash: dash)))
  let items = (
    (redline-ins-rules([text], false, st: st), [inserted text]),
    (redline-del-rules(st: st)[text], [deleted text]),
    (frame(st.ins-frame), [inserted block]),
    (frame(st.del-frame), [deleted block]),
    ..if not steps { ((text(fill: st.ins)[Text], [new paragraph]),) },
    ([#bar(st.ins, st.bar-width, none)#h(3pt)#bar(st.del, st.del-bar-width, (2.5pt, 1.5pt))], [changed lines]),
    (text(fill: redline-style.number)[3], [change number]),
  )
  if info.at("moved", default: 0) > 0 {
    items += (
      ([#redline-move-bracket#text(fill: st.ins, size: 0.8em)[moved]], [new place of moved text]),
      (text(fill: redline-style.move, size: 0.85em)[#text(fill: st.del, sym.arrow.b) Moved …], [its old place]),
    )
  }
  if steps { items.push(([], [colour: the #if "groups" in info.history [step] else [commit] that made the change])) }
  set text(size: 0.9em)
  set par(leading: 0.5em)
  block(breakable: false, grid(
    columns: (auto, 1fr, auto, 1fr),
    column-gutter: (0.6em, 1.2em, 0.6em),
    row-gutter: 0.7em,
    ..items.map(((mark, words)) => (align(right, mark), text(fill: luma(110), words))).flatten(),
  ))
}

// The summary's pages: as wide as the document's but wide enough for lines
// of 25 em, in the document's font at 9 to 12 pt, with margins in the
// proportion of Typst's default and none of the document's page decoration.
// A sheet is as high as the document's page or a portrait sheet of that
// width, whichever is higher. The summary takes one page, a sheet high or
// as high as it needs up to one and a half sheets, rather than leave a few
// lines on a page of their own; a longer one continues on pages a sheet
// high. Typst 0.11 adds a blank page after a page set rule
// in a context; there the summary takes the document's pages.
#let redline-summary-page(body) = if sys.version < version(0, 12, 0) {
  set page(header: none, footer: none, numbering: none)
  body
} else {
  context {
    let size = calc.min(calc.max(text.size.to-absolute(), 9pt), 12pt)
    let (w, h) = if page.flipped { (page.height, page.width) } else { (page.width, page.height) }
    let w = calc.max(if w == auto { 21cm } else { w }, 25 * size * 21 / 16)
    let margin = 2.5 / 21 * w
    let h = if h == auto { 0pt } else { h }
    let sheet = calc.max(h, calc.sqrt(2) * w)
    let fits = measure(block(width: w - 2 * margin, text(size: size, fill: black, body))).height <= 1.5 * sheet - 2 * margin
    set page(width: w, height: if fits { auto } else { sheet }, margin: margin, flipped: false, columns: 1, fill: none, header: none, footer: none, numbering: none, foreground: none, background: none)
    set text(size: size, fill: black)
    // The empty column keeps the page as high as the document's.
    if fits { grid(columns: (0pt, 1fr), block(height: calc.max(0pt, h - 2 * margin)), body) } else { body }
  }
}

// The new side of the side-by-side layout ends with the summary too, which
// the layout shows after the pages.
#let redline-summary(info) = if redline-side != "old" {
  let st = redline-style
  pagebreak(weak: true)
  redline-summary-page(context {
    let shown = redline-laid-out()
    redline-layout()
    let nums = redline-numbering(info.entries)
    redline-meta(nums, <redline-numbers>)
    let bib = info.at("bib", default: (added: (), removed: (), changed: ()))
    let listed = ("added", "removed", "changed", "files").map(k => bib.at(k, default: ()).len()).sum()
    let lost = info.entries.map(e => redline-lost(e.marks, shown))
    let lost-ids = lost.flatten().map(m => m.id)
    if lost-ids.len() > 0 { redline-ints(lost-ids, <redline-lost>) }
    let lost = lost.map(redline-joined)
    let unmarked = listed + info.entries.map(e => e.code.len()).sum(default: 0)
    // The parts of a change count as one.
    let counted(l) = l.filter(m => not m.at("joined", default: false)).len()
    let apart = unmarked + lost.map(counted).sum(default: 0)
    let total = info.entries.map(e => e.count).sum(default: 0) + info.at("moved", default: 0) + listed
    let unchecked = bib.at("unchecked", default: false)
    redline-meta((unmarked: apart, notes: redline-fallback-note != none or unchecked, empty: total == 0), <redline-summary>)
    // English, whatever language the document is in.
    set text(lang: "en", dir: ltr)
    set par(justify: false)
    block(text(size: 1.4em, weight: "bold")[Changes from #info.old to #info.new])
    if total == 0 [No changes found between the two versions.] else [#total #if total == 1 [change] else [changes]#if apart == total [, not marked on the pages but listed below] else if apart > 0 [, #apart of them not marked on the pages but listed below].]

    // Pages by their number in this PDF, which is the number printed on
    // them unless a page prints another.
    let printed = query(<redline-page>).all(m => redline-printed(m.location()) in (none, str(m.location().page())))
    let rows = redline-section-rows(info, lost, nums)
    let min-level = calc.min(..rows.map(r => r.level), 9)
    let rows = rows.map(r => {
      let pages = if r.pages.len() > 0 { redline-pages(r.pages) }
      let (name, pages) = if r.heading == none { (r.name, pages) } else { (link(r.heading, r.name), link(r.heading, pages)) }
      (pad(left: (r.level - min-level) * 1.2em, name), if r.ns.len() > 0 { redline-number(r.ns) }, r.what, pages)
    }).flatten()
    if rows.len() > 0 {
      redline-head[Changes by section]
      let grey = text.with(size: 0.85em, fill: luma(110))
      let head = (grey[Section], grey[No.], grey[Changes], grey(if printed [Page] else [PDF page]))
      // Columns are as wide as their content. Section names get their width
      // first, as long as the changes keep a third of what the numbers and
      // pages leave; neither gets less than its longest unbreakable part,
      // such as a number and its word (Typst 0.11 cannot measure that).
      layout(size => {
        let gutter = 1em.to-absolute()
        let widest(k, ..room) = calc.max(..(head + rows).enumerate().filter(((i, _)) => calc.rem(i, 4) == k).map(((_, c)) => measure([#c], ..room).width))
        let room = size.width - widest(1) - widest(3) - 3 * gutter
        let least(k) = if sys.version >= version(0, 12, 0) { widest(k, width: 1pt) } else { 0pt }
        let section = calc.max(least(0), calc.min(widest(0), room - calc.min(widest(2), calc.max(room / 3, least(2)))))
        grid(
          columns: (section, auto, calc.max(least(2), calc.min(widest(2), room - section)), auto),
          column-gutter: gutter,
          row-gutter: 0.6em,
          grid.header(..head),
          ..rows,
        )
      })
    }

    // Changes marked only by a bar, such as a blank line that spaces items
    // apart, said in words, and deleted code that draws nothing.
    let unseen = query(<redline-unseen>).map(redline-value).flatten()
    let bars = info.entries.map(e => (title: e.title, bars: e.at("bars", default: ()) + e.marks.filter(m => m.at("id", default: none) in unseen))).filter(e => e.bars.len() > 0)
    if bars.len() > 0 {
      redline-head[Marked with a bar only]
      for e in bars {
        block(below: 0.9em, { block(below: 0.55em, ..redline-sticky, emph(e.title)); pad(left: 1.2em, e.bars.map(c => redline-code(c, n: nums.at(str(c.id), default: none))).join(v(0.3em))) })
      }
    }

    let groups = info.entries.zip(lost).filter(((e, l)) => e.code.len() + l.len() > 0)
    redline-not-marked(groups.map(((e, l)) => (if e.deleted { strike(e.title) } else { e.title }, e.code + l)), bib, nums: nums)

    context {
      let notes = ()
      if redline-fallback-note != none { notes.push(redline-fallback-note) }
      let unbarred = query(<redline-unbarred>).map(redline-value).flatten()
      if unbarred.len() > 0 {
        let at = query(selector(<redline-tag>).or(<redline-move>)).map(t => t.location()).filter(l => l.page() in unbarred)
        notes.push[Changes on #redline-page-names(at) have no bars in the margin, because the document sets its own page foreground and background there.]
      }
      let states = query(<redline-unrestored>)
      if states.len() > 0 {
        let keys = states.map(redline-value).flatten().dedup()
        let names = keys.map(k => if k == page [the page counter] else { raw(k) }).join(", ")
        let (it, that) = if keys.len() == 1 { ([a state], [that state]) } else { ([states], [those states]) }
        notes.push[Deleted content on #redline-page-names(states.map(m => m.location())) updates #it (#names). What reads #that afterwards can differ from the new version.]
      }
      if unchecked { notes.push[Files that the document reads through a computed path were not checked for changes; that needs Typst 0.14 or later.] }
      redline-notes(notes)
    }

    redline-commits(info.at("history", default: none), info.old, info.new)
    // The side-by-side layout says how it marks changes on its first page.
    if total > 0 and redline-side == none {
      redline-head[How changes are marked]
      redline-marks-legend(info)
    }
    redline-meta(none, <redline-summary-end>)
  })
} else {
  context {
    redline-layout()
    redline-meta(redline-numbering(info.entries), <redline-numbers>)
    let lost = redline-lost(info.entries.map(e => e.marks).flatten(), redline-laid-out())
    if lost.len() > 0 { redline-ints(lost.map(m => m.id), <redline-lost>) }
  }
}

// The side-by-side layout. `info` is (old, new: (label, pdf: the version
// compiled with `redline-side`, pages: before the summary, summary: its
// pages after them, unbarred: pages with changes but no
// bars), pairs: array of (old, new: pages
// shown next to each other, steps-old, steps-new: pages of earlier steps of
// the same slide, not shown, moved: whether they hold both places of moved text), unpaired: array of
// (side, ins, page): changes
// found in one version only, unmarked: array of (file, old, new): changes
// that are not marked and not in the summary, moved: whether text moved).
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
  let name(side) = text(weight: "bold", fill: if side == o { st.del } else { st.ins }, side.label)
  let left-out(side, ps) = side.pages - ps.map(p => p.len()).sum(default: 0)
  // The page of one side of a pair shown in row `r`: a side with fewer pages
  // shows its last one again, next to the other pages it shares changes with.
  let row-page(ps, r) = if ps.len() > 0 { ps.at(calc.min(r, ps.len() - 1)) }
  let hist = info.at("history", default: none)
  let steps = hist != none and hist.steps
  let sample = if steps { st + redline-colours(luma(70)) } else { st }
  set page(width: auto, height: auto, margin: gap)
  set text(size: size)
  // The first page is as large as what it says.
  block(width: calc.min(width + gap, 36 * size), {
    block(text(size: 1.4em, weight: "bold")[Changes from #o.label to #n.label])
    let (lo, ln) = (left-out(o, info.pairs.map(p => p.old + p.steps-old)), left-out(n, info.pairs.map(p => p.new + p.steps-new)))
    let shown = info.pairs.map(p => calc.max(p.old.len(), p.new.len())).sum(default: 0)
    let summary = if n.summary > 0 [#link((page: 2 + shown, x: 0pt, y: 0pt), underline[page #(2 + shown)])]
    if info.pairs.len() == 0 [No page differs between the two versions#if summary != none [; the summary of the changes is on #summary].] else [
      Pages of #name(o) on the left, with #redline-del-rules(st: sample)[deleted] text; pages of #name(n) on the right, with #redline-ins-rules([inserted], false, st: sample) text#if steps [, in the colour of the #if "groups" in hist [step] else [commit] that made the change]. Bars in the margin mark the changed lines.#if info.at("moved", default: false) [ Moved text has a grey bracket #redline-move-bracket and a #text(size: 0.8em)[moved] note in the margin on both sides, and its old page is shown next to its new one; edits made while moving are marked as usual.]
      #if lo + ln > 0 [Pages without changes are left out: #ln of the #n.pages pages of #n.label and #lo of the #o.pages of #o.label.]
      #if summary != none [The summary of the changes follows on #summary.]
    ]
    // One row per shown page.
    let at = 2
    let rows = ()
    for p in info.pairs {
      let k = calc.max(p.old.len(), p.new.len())
      for r in range(k) {
        let cell(ps, steps) = {
          let pg = row-page(ps, r)
          if pg == none [–] else [#pg#if r == k - 1 and steps.len() > 0 [ (last step of #pages(steps + ps))]]
        }
        let moved = if p.moved [ #text(fill: st.move)[(moved)]]
        rows += (cell(p.old, p.steps-old), [#cell(p.new, p.steps-new)#moved], link((page: at + r, x: 0pt, y: 0pt), underline[#(at + r)]))
      }
      at += k
    }
    if rows.len() > 0 {
      let head = ([Page of #name(o)], [Page of #name(n)], [Shown on page])
      table(
        columns: head.len(),
        stroke: none,
        inset: (x: 0.5em, y: 0.3em),
        align: (left, left, right),
        table.header(..head.map(strong)),
        table.hline(stroke: 0.5pt),
        ..rows,
      )
    }
    if info.unpaired.len() > 0 {
      [Changes found in one version only; their pages are shown without a counterpart:]
      // An insertion is marked in the old version where it would be, a
      // deletion in the new one.
      list(..info.unpaired.map(u => {
        let what = if (u.side == "new") != u.ins [The place of #if u.ins [an insertion] else [a deletion]] else if u.ins [An insertion] else [A deletion]
        [#what on page #u.page of #info.at(u.side).label]
      }))
    }
    // The summary says the rest, of the new version.
    let files = info.unmarked.map(u => u.file).dedup()
    redline-not-marked(files.map(f => ([In #redline-mono(f)], info.unmarked.filter(u => u.file == f))), (added: (), removed: (), changed: ()))
    if o.unbarred.len() > 0 { redline-notes(([Changes on pages #pages(o.unbarred) of #name(o) have no bars in the margin, because the document sets its own page foreground and background there.],)) }
  })
  let head(side, pg, s, ps) = if pg != none [#name(side), page #pg#if s.len() > 0 [, the last step of #pages(s + (pg,))]] else if ps.len() == 0 [Not in #name(side)]
  // A side without a page shows an empty outline as large as the page next
  // to it, so that the other stays in place.
  let shot(side, w, pg, other) = if pg != none { box(stroke: 0.5pt + luma(150), image(side.pdf, page: pg)) } else {
    let h = if other.at(1) != none { measure(image(other.at(0).pdf, page: other.at(1))).height } else { 0pt }
    box(width: w, height: h, stroke: (paint: luma(150), thickness: 0.5pt, dash: "dashed"))
  }
  let legend = if steps { redline-legend(hist, width + gap, size: 0.8 * size) }
  for p in info.pairs {
    for r in range(calc.max(p.old.len(), p.new.len())) {
      pagebreak()
      let (po, pn) = (row-page(p.old, r), row-page(p.new, r))
      grid(columns: 2, column-gutter: gap, row-gutter: 0.5em, head(o, po, p.steps-old, p.old), head(n, pn, p.steps-new, p.new), shot(o, wo, po, (n, pn)), shot(n, wn, pn, (o, po)), ..if legend != none { (grid.cell(colspan: 2, align(center, legend)),) })
    }
  }
  // The summary's pages, as many next to each other as the width of the
  // pages above holds.
  let ws = if n.summary > 0 { measure(image(n.pdf, page: n.pages + 1)).width } else { wn }
  let across = calc.max(1, calc.floor((width + gap) / (ws + gap)))
  for k in range(0, n.summary, step: across) {
    pagebreak()
    let ks = range(k, calc.min(k + across, n.summary))
    let label(k) = if n.summary == 1 [Summary] else [Summary, page #(k + 1) of #n.summary]
    grid(columns: across, column-gutter: gap, row-gutter: 0.5em, ..ks.map(label), ..ks.map(k => box(stroke: 0.5pt + luma(150), image(n.pdf, page: n.pages + k + 1))))
  }
}
