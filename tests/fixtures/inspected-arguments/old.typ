// A container that takes content and inspects its other arguments, as plot
// and algorithm packages do: each must be an element of its own kind.
#let plot(..ys, color: black) = metadata((ys: ys.pos(), color: color))
#let diagram(title: [], ..plots) = {
  for p in plots.pos() {
    assert(type(p) == content and p.func() == metadata, message: "unexpected child " + repr(p))
  }
  figure(caption: title, rect[#plots.pos().len() plots])
}
Some text.

#diagram(title: [Population], plot(1, 2, 3, color: blue), plot(2, 3, 4))
