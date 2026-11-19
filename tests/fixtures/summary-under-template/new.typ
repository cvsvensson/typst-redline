#set page(width: 12cm, height: auto)
#let template(title: "", body) = {
  show raw.where(block: false): box.with(fill: luma(235), inset: (x: 3pt), outset: (y: 10pt, x: 6pt))
  align(center, text(1.4em, title))
  body
}
#show: template.with(
  title: "Notes about two topics",
)
= Intro
Text with `code`.
