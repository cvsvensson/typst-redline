#let paper(title: "", authors: (), email: "", abstract: [], keywords: (), body) = {
  align(center, text(1.5em, strong(title)))
  align(center, authors.join(", ") + " · " + raw(email))
  block(inset: 1em)[*Abstract.* #abstract]
  [*Keywords:* #keywords.join(", ")]
  body
}
#show: paper.with(
  title: "Marking changes in long documents",
  authors: ("Ada Example", "Bo Sample"),
  email: "ada@example.com",
  abstract: [We mark changes many times in a document.],
  keywords: ("Typesetting", "Diffs"),
)

= Introduction
Some text.
