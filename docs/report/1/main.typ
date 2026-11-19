#import "template.typ": report
#set page(width: 13cm, height: auto, margin: (x: 1.4cm, y: 1.4cm))

#show: report.with(
  title: "Sorting on small devices",
  author: "Robin Lee",
  date: "Draft, November 2026",
  abstract: [We compare three sorting algorithms on a microcontroller with 32 kB of memory. Insertion sort is fastest for short lists.],
)

#include "chapters/intro.typ"
#include "chapters/results.typ"

#bibliography("refs.bib", title: "References")
