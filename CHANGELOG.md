# Changelog

## Unreleased

- Inside a changed formula, only the changed parts are marked: deleted ones crossed out in red, new ones in blue. A formula that changed for the most part, or lost a line break or alignment point, is still shown whole. `--math-markup whole` shows every changed formula whole.

## 0.1.0

First release. `typst-redline` makes a PDF of the new version of a Typst document with the changes against an old version marked.

- Compares two files, or a file against a git revision, two revisions, or a branch since it left another (`--since`, `--to`, or latexdiff-vc's `-r`); `-L` names the versions in the PDF.
- Deleted words are struck through and new words highlighted, also in headings, list items, captions and footnotes; a deleted list item stays in its list and a deleted line break shows as a struck ↵.
- Changed formulas, figures, code blocks and drawings are shown whole, the old one in a dashed frame before the new one; deleted images and plots are crossed out, other deleted blocks tinted. A figure whose body changed keeps its caption once, with edits in it marked.
- Bars in the margin mark changed lines, and each change has a number next to them, in reading order.
- Follows files the document includes or imports. A replaced image is marked at its figure; changed data files and bibliography entries are listed on the summary page.
- Shows moved paragraphs, list items and sections once, with a note and the same number at both places (`--no-moves` turns this off).
- A summary at the end, on pages of its own without the document's header and footer, counts the changes by section with their numbers and pages, lists by number every change it cannot mark in place, such as a changed title string, a `#set` rule or a changed label, and, for git comparisons, the commits. A line at the top of the first page points to it.
- With the deletions removed, the marked document has the new version's text, layout and numbering; the summary's pages do not count in the document's page total.
- `--only-changes` keeps only the pages with marks and the summary, with their printed page numbers.
- `--layout side-by-side` shows each page that a change covers next to its old page, for slides in particular, and ends with the same summary.
- `--each-commit` and `--steps` colour each change by the commit or group of commits that made it.
- `--mark-reviewed` and `--since reviewed` show what changed since a document was last read.
- `--list-files` prints the files a document is made of.
- Run without a file in a terminal, it asks for the file and the version to compare with.
- `--style` changes colours, strokes and other settings of the marks, `-t` starts from a latexdiff markup type, `--graphics-markup` shows changed formulas and figures as latexdiff's option does, and `-p`/`--preamble` replaces the styling whole; `--show-preamble` prints the built-in one.
- Compiles with the `typst` on `PATH`. Tested with Typst 0.15.1 on Linux, macOS and Windows and with 0.14.2 on Linux; the tests also pass with 0.11.1. Typst 0.12 and 0.13 are not tested. Before 0.14, the side-by-side layout does not work and files read through a computed path are not checked.
- A VS Code extension in `vscode/` runs the tool on the open file.
