#set page(width: 8cm, height: 6cm, margin: (inside: 3cm, outside: 1cm, y: 1cm))
#set page(header: context [#metadata((calc.odd(here().page()), counter(page).get().first()))<probe>])
First page words.
#pagebreak()
Second page.
