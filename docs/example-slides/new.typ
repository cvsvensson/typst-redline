#set page(paper: "presentation-16-9", margin: (x: 2cm, y: 1.6cm))
#set text(size: 28pt)
#show heading: set block(below: 1em)
#set list(spacing: 1em)

#align(horizon + center)[
  #text(36pt, weight: "bold")[A short talk]

  This title slide is unchanged, so it is left out.
]

#pagebreak()
= The new slide title

#grid(columns: (1.1fr, 1fr), column-gutter: 1.5cm,
  [- This item is unchanged.
- This item has one inserted word.
- This item is inserted.
- This item is unchanged too.
- This whole item is inserted.],
  [The new slide is shown on the right.

$ x_"new" = 2 $

#rect(width: 100%, height: 4.6cm, radius: 6pt)[#align(center + horizon)[the new box]]],
)

#pagebreak()
= Summary

This slide is unchanged too.
