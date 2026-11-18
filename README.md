# typst-redline

An editor's proof for [Typst](https://typst.app), in the spirit of `latexdiff`: a PDF of the new version of a document with insertions highlighted, deletions struck through, change bars in the margin, and a first page listing the changed sections with links.

Both versions are parsed with Typst's own parser (`typst-syntax`), so changes are found at the level of paragraphs, headings, list items and equations, then word by word inside prose. Inline and display math, references, labels and code are never split: a changed formula shows whole, old struck and new highlighted. Only the new version's labels exist in the output, so every `@ref` still resolves; labels in deleted text are dropped and references to deleted targets become plain text. Line endings are normalized, so a CRLF working copy against LF from git shows no changes.

## Install

```sh
cargo install --path .
```

Needs the `typst` CLI on `PATH` (tested with 0.15.1).

## Usage

```sh
# changes in the working copy since a git revision
typst-redline notes/notes.typ --since HEAD~3 --root .

# between two revisions
typst-redline notes/notes.typ --since v1 --to v2 --root .

# between two files
typst-redline old.typ new.typ
```

The result is `<name>.redline.pdf` next to the file (`-o` to change). The generated source is written next to the new file as `<name>.redline-tmp.typ`, so that relative imports, images and the bibliography resolve, and deleted after compiling unless `--keep-typ` is given. `--root` is passed to `typst compile`.

Colors and bar position are set in a `#let redline-style = (...)` dictionary at the top of the built-in [preamble](src/preamble.typ). To change them, copy that file, edit it, and pass `--preamble my-style.typ`.

## VS Code

`vscode/` holds a minimal extension with the command *Typst Redline: Show changes since…*: it lists recent commits of the active `.typ` file, runs `typst-redline`, and opens the PDF. Build it with `npm install && npm run compile` in `vscode/`, then package it with `vsce` or load the folder as a development extension.

## Limitations

- Bold and emphasized phrases, and code expressions such as `#figure(...)`, are compared as wholes: a one-word change inside them marks the whole element.
- Moved text shows as deleted in one place and inserted in another.
- Changes to `#set`, `#show`, `#let` and `#import` lines are not marked; the new version's are used. Deleted code that uses a removed definition is shown as source text.
- Imports, figures and the bibliography are read from the working tree, so comparing two old revisions fails if they cite entries that are no longer in the bibliography.
- A change bar marks the first and last line of a changed run inside a paragraph, not every line in between.

## License

MIT. The dependencies `typst-syntax` and `similar` are Apache-2.0, which is compatible with an MIT-licensed project; a distributed binary must carry their license notices.
