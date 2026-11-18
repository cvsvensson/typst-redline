#let thm = counter("thm")
#show figure.where(kind: "thm"): it => block[#thm.step()*Theorem #context thm.display().* #it.body]
#let theorem(body) = figure(kind: "thm", supplement: [Theorem], body)
#let notefigure(body) = context block[#body Figure #counter(figure.where(kind: image)).display(figure.numbering)]

#theorem[First.]

#theorem[Second.]

#notefigure[A sketch.]

#theorem[Third.]

#context [#metadata(thm.get()) <probe>]
