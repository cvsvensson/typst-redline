# Changelog

## 0.1.0

First release. `typst-redline` makes a PDF of the new version of a Typst document with the changes against an old version marked.

- Compares two files, or a file against a git revision, two revisions, or a branch since it left another (`--since`, `--to`).
- Deleted words are struck through, new words highlighted; changed formulas, figures, code blocks and drawings are framed whole; bars in the margin point to changed lines.
- Follows files the document includes or imports, and lists changed images, data and bibliography entries on the summary page.
- Shows moved paragraphs, list items and sections once, with a note at both places (`--no-moves` turns this off).
- Lists every change it cannot mark in place, such as changed code or text a function reads as text, on a summary page at the end, with the changed sections and, for git comparisons, the commits.
- With the deletions removed, the marked document has the new version's text, layout and numbering.
- `--layout side-by-side` shows each changed page of the old version next to the new one, for slides in particular.
- `--each-commit` and `--steps` colour each change by the commit or group of commits that made it.
- `--mark-reviewed` and `--since reviewed` show what changed since a document was last read.
- Compiles with the `typst` on `PATH`. Tested with Typst 0.15.1 on Linux, macOS and Windows and with 0.14.2 on Linux; with 0.11.1 all tests pass but one on the page numbers of a two-sided document. Typst 0.12 and 0.13 are not tested. Before 0.14, the side-by-side layout does not work and files read through a computed path are not checked.
- A VS Code extension in `vscode/` runs the tool on the open file.
