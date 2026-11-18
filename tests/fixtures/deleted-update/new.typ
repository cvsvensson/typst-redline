#set heading(numbering: "1.")
#let st = state("s", 0)

= Head

Now.

#context [#metadata((st.get(), counter(heading).get().first())) <probe>]
