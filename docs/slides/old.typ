#let navy = rgb("#1f3b57")
#set page(width: 16cm, height: 9cm, margin: (x: 1.2cm, top: 2.2cm, bottom: 1cm),
  header: context {
    let here-page = here().page()
    let title = query(heading.where(level: 1)).find(h => h.location().page() == here-page)
    place(top + left, dx: -1.2cm, rect(width: 16cm, height: 1.5cm, fill: navy, inset: (x: 1.2cm),
      align(horizon, text(fill: white, size: 18pt, weight: "bold", if title != none { title.body }))))
  },
  footer: context align(right, text(size: 9pt, fill: gray, counter(page).display())))
#set text(size: 15pt)
#show heading.where(level: 1): it => pagebreak(weak: true)

#let bars(..values) = grid(
  columns: values.pos().len(), column-gutter: 0.4cm, row-gutter: 0.15cm, align: center + bottom,
  ..values.pos().map(((name, v)) => rect(width: 1.2cm, height: v * 0.4cm, fill: navy.lighten(30%))),
  ..values.pos().map(((name, v)) => text(size: 10pt, name)),
)

= Sorting on small devices
Robin Lee \
Group meeting, 4 November 2026

= Why it matters
- A sensor node sorts its readings to send the median
- Memory is a few kilobytes
- No room for a second copy of the data

= Results
- Insertion sort is fastest below 30 numbers
- Heap sort is fastest above that
- Merge sort needs too much memory

#align(center, bars(("insertion", 6), ("heap", 3), ("merge", 4)))

= Next steps
- Measure energy, not only time
- Try lists that are almost sorted
