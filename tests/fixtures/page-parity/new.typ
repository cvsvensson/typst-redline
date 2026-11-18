#set page(width: 8cm, height: 6cm)
#set page(header: context [#metadata((calc.odd(here().page()), counter(page).get().first()))<probe>])
First page words.
#pagebreak()
Second page.
