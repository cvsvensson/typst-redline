#let note(body) = [#metadata(body)#super[n]]
Some old text#note[first] and more#note[second]#note[third].

#context [#metadata(query(metadata).len())<probe>]
