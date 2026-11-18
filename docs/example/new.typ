#import "@preview/cetz:0.4.2": canvas, draw
#set page(width: 14cm, height: auto, margin: 1.4cm)
#set text(size: 10pt)
#set par(justify: true)

= The new title

#grid(columns: (1fr, 1fr), column-gutter: 1cm,
  [This sentence has one inserted word. The bars in the margin point to the changed lines.

This formula is unchanged:
$ a^2 + b^2 = c^2 $
This one is changed:
$ x_"new" = 2 $],
  [#figure(
    canvas(length: 0.5cm, {
      import draw: *
      circle((0, 0), radius: (2.6, 1.5))
      content((0, 0), [new drawing])
    }),
    caption: [The new drawing.],
  )

This is an inserted paragraph. The marks on this page are made by #link("https://github.com/cvsvensson/typst-redline")[`typst-redline`].],
)
