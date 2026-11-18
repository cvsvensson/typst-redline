#let note(body) = [#metadata(body)#super[n]]
Some new text#note[first] and more#note[second].

#context [#metadata(query(metadata).len())<probe>]
