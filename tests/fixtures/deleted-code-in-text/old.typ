#let total = state("t", 0)
Alpha beta gamma.
#total.update(x => x + 5)
Delta epsilon.
#counter("c").step()
Zeta eta.
#metadata("x") <m>
The total is #context total.get(), the count #context counter("c").get().first().
