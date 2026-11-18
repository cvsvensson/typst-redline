// Cells may be dictionaries made by a helper, as older table packages do.
#let cellx(fill: none, body) = (kind: "cell", fill: fill, body: body)
#let tablex(..cells) = table(columns: 2, ..cells.pos().map(c => if type(c) == dictionary { table.cell(fill: c.fill, colspan: 2, c.body) } else { c }))
#tablex([A], [B], cellx[Summary])
