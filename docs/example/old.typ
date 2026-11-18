#import "@preview/cetz:0.4.2": canvas, draw
#set page(width: 14cm, height: auto, margin: 1.4cm)
#set text(size: 10pt)
#set par(justify: true)

= The old title

#grid(columns: (1fr, 1fr), column-gutter: 1cm,
  [This sentence has one deleted word. The bars in the margin point to the changed lines.

This is a deleted paragraph.

This formula is unchanged:
$ a^2 + b^2 = c^2 $
This one is changed:
$ x_"old" = 1 $],
  [#figure(
    canvas(length: 0.5cm, {
      import draw: *
      rect((-2.4, -1.3), (2.4, 1.3))
      content((0, 0), [old drawing])
    }),
    caption: [The old drawing.],
  )],
)
