#set heading(numbering: "1.")
#let st = state("s", 0)

#st.update(5)

= Head

#counter(heading).update(4)

Now.

#context [#metadata((st.get(), counter(heading).get().first())) <probe>]
