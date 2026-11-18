The pictures `docs/example-commits-summary.png` and `docs/example-commits.png` are pages 1 and 2 of the result for this document.
To make them, commit `v0.typ` as `main.typ` in a new git repository, then commit `v1.typ` ("Add a sentence"), `v2.typ` ("Change a word and a formula") and `v3.typ` ("Edit two paragraphs") over it in that order.
Then run `typst-redline main.typ --since <first commit> --each-commit --keep-typ` and `typst compile --root <kept> --format png --ppi 190 <kept>/main.typ out-{p}.png`.
