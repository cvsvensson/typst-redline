#import "@preview/cetz:0.4.2": canvas, draw
#set page(width: 14cm, height: auto, margin: 1.4cm)
#set text(size: 10pt)

= An example

#grid(columns: (1fr, 1fr), column-gutter: 1cm,
  [This sentence has one inserted word. The bars in the margin point to the changed lines.

This formula is changed:
$ x_"new" = 2 $

This is an inserted paragraph. The marks on this page are made by #link("https://github.com/cvsvensson/typst-redline")[`typst-redline`].],
  [#figure(
    canvas(length: 0.5cm, {
      import draw: *
      circle((0, 0), radius: (2.6, 1.5))
      content((0, 0), [new drawing])
    }),
    caption: [A changed drawing.],
  )],
)
