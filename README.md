# typst-redline

`typst-redline` shows what changed between two versions of a [Typst](https://typst.app) document: a PDF of the new version with deleted text struck out and new text highlighted. It does for Typst what `latexdiff` does for LaTeX. AI agents wrote this tool.

<img src="docs/example.png" width="480" alt="A paragraph and a formula with changes marked">


## Install

You need [Typst](https://github.com/typst/typst#installation), [git](https://git-scm.com/downloads) and [Rust](https://rustup.rs). Then:

```sh
cargo install --git https://github.com/cvsvensson/typst-redline
```

## Use

```sh
typst-redline paper.typ                    # changes since the last commit
typst-redline paper.typ --since HEAD~3     # changes since an earlier commit
typst-redline paper.typ --since v1..v2     # changes between two commits
typst-redline old.typ new.typ              # changes between two files, no git needed
```

The result is `paper.redline.pdf` next to your file; `-o out.pdf` puts it elsewhere. Anything git accepts works after `--since`: a commit hash, a tag, a branch name.

If your document uses files outside its own folder, such as `#import "../macros.typ"`, pass the project folder with `--root`, as you would to `typst compile`. `typst-redline --help` lists the other options.

## What is marked

Every change to the text of the document is marked where it appears, also in files the document includes or imports. Formulas are shown whole: the old one struck, the new one highlighted. The first page lists the sections that changed.

Changes to code, such as a function body or a `#set` rule, cannot be shown as marks. The first page lists them as *not marked*, so no change goes unreported.

Known problems:

- A one-word change inside bold or emphasized text marks the whole phrase.
- Moved text shows as deleted in one place and inserted in another.
- Deleted code that no longer works in the new document, for example because it uses a definition you removed, is shown as source text instead of its output.
- Show rules on `strike`, `highlight` or `text` in your document also restyle the marks.

## Styling

Colours and other settings are in the `redline-style` dictionary at the top of [src/preamble.typ](src/preamble.typ). Copy that file, edit it, and pass it with `--preamble my-style.typ`.

## VS Code

The extension in `vscode/` adds the command *Typst Redline: Show changes since…*: pick a commit of the open file and it opens the PDF. It needs `typst-redline` installed as above. Build it with `npm install && npm run package` in `vscode/` and install the resulting `.vsix` with *Extensions: Install from VSIX…*.

## Claude Code

[`claude-plugin/`](claude-plugin) is a plugin with a skill for compiling a Typst document and making its redline, for requests like *"send me the notes and what changed since Monday"*. Install it with

```
/plugin marketplace add cvsvensson/typst-redline
/plugin install typst-redline@typst-redline
```

or enable it for a project in its `.claude/settings.json`:

```json
{
  "extraKnownMarketplaces": {
    "typst-redline": { "source": { "source": "github", "repo": "cvsvensson/typst-redline" } }
  },
  "enabledPlugins": { "typst-redline@typst-redline": true }
}
```

## See also

[typdiff](https://github.com/sou1118/typdiff) and [typst-diff](https://github.com/alexanderkoller/typst-diff) are other tools for the same job.

## License

MIT
