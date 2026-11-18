// A template that reads its body, as slide packages do.
#let template(body) = {
  let parts = if body.has("children") { body.children } else { (body,) }
  assert(parts.all(c => repr(c.func()) != "context"), message: "the body is hidden")
  body
}
#show: template

Old text.
