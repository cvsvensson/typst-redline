#set page(width: 12cm, height: 8cm, margin: 1cm)
#show box: it => [#metadata(it.width)<probe>#it]
Before.

#grid(columns: (1fr, 1fr), column-gutter: 1cm, [A left cell with text that wraps.], [Right.])
