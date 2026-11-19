#let report(title: none, author: none, date: none, abstract: none, body) = {
  set document(title: title, author: author)
  set text(size: 10pt)
  set par(justify: true)
  set heading(numbering: "1.")
  show heading.where(level: 1): it => {
    pagebreak(weak: true)
    v(1.2em)
    it
    v(0.4em)
  }
  align(center)[
    #v(1.5cm)
    #text(17pt, weight: "bold", title)
    #v(0.5em)
    #author \
    #text(gray.darken(40%), date)
  ]
  v(1cm)
  pad(x: 0.6cm)[
    #align(center, smallcaps[Abstract])
    #abstract
  ]
  set page(numbering: "1", number-align: top + right)
  body
}
