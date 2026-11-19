#set page(width: 12cm, height: 9cm, margin: (x: 1.4cm, y: 1.2cm))
#set text(size: 10pt)
#set par(justify: true)
#set heading(numbering: "1.")
#counter(heading).update(2)

= Related work

Hoare's quicksort#footnote[C. A. R. Hoare, "Quicksort", _The Computer Journal_ 5, 1962.] sorts in place but needs a stack whose depth grows with the input. Sensor nodes rarely have room for it, so most firmware falls back to insertion sort.

Heap sort needs no stack and runs in $O(n log n)$ time in the worst case.#footnote[J. W. J. Williams, "Algorithm 232: Heapsort", _Communications of the ACM_ 7, 1964.] It is seldom used on small lists, where its constant factor dominates.
