#let node(body, name: none, fill: none) = (name: name, body: box(fill: fill, inset: 3pt, body))
#let edge(from, to) = (from: from, to: to)
#let diagram(..items) = {
  let names = items.pos().filter(i => "name" in i).map(i => i.name)
  for e in items.pos().filter(i => "from" in i) {
    assert(e.from in names and e.to in names, message: "unknown node")
  }
  items.pos().filter(i => "name" in i).map(i => i.body).join(sym.arrow)
}

#diagram(
  node([Client], name: <client>),
  node([Server], name: <server>, fill: yellow),
  edge(<client>, <server>),
)
