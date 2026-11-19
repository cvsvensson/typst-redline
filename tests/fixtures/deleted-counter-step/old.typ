#set page(width: 12cm, height: auto)
// Theorems numbered by a counter of their own, stepped when the element is
// shown, as theorem packages do.
#let thm = counter("thm")
#show figure.where(kind: "thm"): it => block[#thm.step()*Theorem #context thm.display().* #it.body]
#let theorem(body) = figure(kind: "thm", supplement: [Theorem], body)

#theorem[First.]

#theorem[Second, to be deleted.]

#theorem[Third.]
