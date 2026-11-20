# typst-redline

Like latexdiff, for Typst.

`typst-redline` shows what changed between two versions of a [Typst](https://typst.app) document. It makes a PDF of the new version with deleted text struck out, new text highlighted, changed formulas and figures framed, and bars and change numbers in the margin; a summary at the end lists every change, including those it cannot mark on the page. AI agents wrote this tool.

<img src="docs/paper.png" width="480" alt="A paper page: a deleted word struck out in red, new words highlighted in blue, the old formula tinted and the old plot crossed out in dashed red frames above the new ones in blue frames, a new paragraph in blue, and bars in the margin next to each changed line, with the number of each change beside them">

The [gallery](docs/gallery.md) shows every kind of mark, each with the files it was made from.

## Install

`typst-redline` compiles with the [`typst`](https://github.com/typst/typst#installation) on your `PATH`, and uses [git](https://git-scm.com/downloads) for comparisons with commits; comparing two files needs no git.

No release has been published yet, so install it with [Rust](https://rustup.rs):

```
cargo install --git https://github.com/cvsvensson/typst-redline
```

Run the same command with `--force` to update, and `cargo uninstall typst-redline` to remove it; the [`cargo install` documentation](https://doc.rust-lang.org/cargo/commands/cargo-install.html) says where it puts the program. `typst-redline --version` checks that it is found.

CI runs the tests with Typst 0.15.1 on Linux, macOS and Windows, and with Typst 0.14.2 on Linux ([ci.yml](.github/workflows/ci.yml)); [Known problems](docs/known-problems.md#typst-versions) says what is known about other versions.

## Use

In a git repository:

```
typst-redline paper.typ                    # uncommitted changes, since the last commit
typst-redline paper.typ --since HEAD~3     # changes since three commits ago
typst-redline paper.typ --since v1         # changes since a tag, branch or commit hash
typst-redline paper.typ --since v1..v2     # changes between two commits
```

Run `typst-redline` without a file in a terminal and it asks which file and which version to compare with, then prints the command it runs and runs it. Files that another file in the folder includes come last, and with one document there is no file question. The versions are the last reviewed version, the changes since the last commit, and the newest commits that change the document's files, each where it applies:

```
Using paper.typ, the only Typst document in this folder.
typst-redline --help lists the options; q quits.
Show the changes to paper.typ.
A commit stands for its own change and every change after it.
   1  the last reviewed version
   2  changes since the last commit
   3  8e8568b  2026-11-19  Revise the body
   4  4dc47c8  2026-11-19  Expand the introduction
Number [1]: 4
typst-redline paper.typ --since 833bb8f
```

Enter takes the first entry and `q` quits. Other options, such as `--layout side-by-side`, apply as given.

Without git, compare two copies; quote paths that contain spaces:

```
typst-redline "thesis v1/thesis.typ" "thesis v2/thesis.typ"
```

The PDF is written next to the file, or next to the new file when you give two, as `<name>.redline.pdf`, and its path is printed. `-o out.pdf` writes it elsewhere. With one file, the new version is the file as it is on disk, uncommitted changes included, unless `--to <revision>` names another.

If the document reads files outside its own folder, such as `#include "../shared/intro.typ"`, pass the folder that holds all of its files with `--root`, for example `typst-redline thesis/main.typ --root .` from that folder. Without `--root`, the `TYPST_ROOT` environment variable is used if it is set, and otherwise the folder of the file. `--typst <path>` picks another `typst` than the one on `PATH`, and arguments after `--` go to `typst compile`, for example `-- --font-path fonts`. `--keep-typ` keeps the marked Typst source and prints where it is, and `--list-files` prints the paths of the files the document is made of. `typst-redline --help` lists every option.

## What is marked

- Deleted text is struck out in red and new text highlighted in blue, also in headings, lists, captions and footnotes.
- A changed formula, figure, table, code block or drawing is shown whole: the old one in a dashed red frame, the new one in a blue frame.
- A bar in the margin points to every changed line, dashed for deletions and solid for insertions, with a number for each change that the summary page uses too.
- A moved paragraph or section is shown once, at its new place, with a "moved" note; at its old place a grey line links to it.
- Changes in included files and replaced images are marked; with git, the old version of each file comes from the old commit.
- What cannot carry a mark, such as a title given as a string, a `#set` rule, a data file or a bibliography entry, is listed on the summary page.

The [gallery](docs/gallery.md) has a picture and the details of each. With the deleted parts removed, the marked PDF has the same text, layout and numbering as the new version; [Known problems](docs/known-problems.md#deleted-content-in-the-new-layout) lists the exceptions.

## The summary page

The last page, or pages, list every change by its number, section by section, and say in words what is not marked on the pages, with its source. A line at the top of the first page, where its top margin has room for it, says where the summary is and how many changes it lists that are not marked on the pages. The [gallery](docs/gallery.md#the-summary-page) describes each part.

<img src="docs/report-summary.png" width="480" alt="The summary page: a sentence with the number of changes, a table of changed sections with the numbers of their changes, counts and pages, a changed title shown as old and new text, a new bibliography entry, the commits in the range, and the legend">

## Only the changed pages

`--only-changes` leaves out the pages without a mark and keeps the summary, for a long document with a few changes. The pages keep the numbers printed on them, and the summary and margin notes name pages by those numbers; a page without a printed number is named by its place in the full PDF, such as *page 8 of the full PDF*. It needs Typst 0.12 or later.

```
typst-redline thesis.typ --since sent-to-anna --only-changes
```

## Slides and side by side

`--layout side-by-side` shows each changed page of the old version next to the same page of the new version, leaves out pages without changes, and ends with the summary. Use it for slides: in the default layout, deleted content takes room on the page, so a full slide overflows. It needs Typst 0.14 or later and an old version that compiles; the default layout falls back to coarser marks instead.

```
typst-redline talk.typ --since HEAD~1 --layout side-by-side
```

<img src="docs/slides.png" width="800" alt="An old slide with a deleted list item and the old bar chart tinted in a dashed frame, next to the new slide with the new list item and the new chart in a blue frame">

## Recipes with git

- **Send a co-author what changed since the version they read**: tag the commit you sent, `git tag sent-to-anna`, and later run `typst-redline paper.typ --since sent-to-anna`.
- **What changed since you last read it**: run `typst-redline paper.typ --mark-reviewed` when you have read it, and later `typst-redline paper.typ --since reviewed`.
- **Who changed what**: `typst-redline paper.typ --since sent-to-anna --each-commit` gives each of up to six commits its own colour ([picture](docs/gallery.md#who-changed-what)).

[docs/git.md](docs/git.md) has the details of these, and recipes for a date, a branch and more than six commits.

## Known problems

The ones you are most likely to meet; [docs/known-problems.md](docs/known-problems.md) has the full list, each tried with the current version.

- In the default layout, deleted content takes room on the page, so line and page breaks shift, and on a full slide the old and the new version of a figure each take a page of their own; use side by side for slides.
- Some moved text shows as deleted at one place and inserted at the other, for example a sentence moved within or between paragraphs, or a passage rewritten so much that it keeps less than about 60% of its words.
- Where a function reads its content as text, a changed text can be missing from the page, not only its mark, and a function that fails on marked content makes the whole document fall back to coarser marks; the summary page says so in both cases.
- Deleted code that uses a name which meant something else at its place, such as a loop variable or a function parameter, is shown by that name, such as *[unit]*, not as it looked.
- Deleted content is laid out under the new version's show and set rules, and a reference in it to a label that the new version lacks shows the label, not the number the old version printed.
- Typst 0.12 and 0.13 are not tested; before Typst 0.14, side by side does not work and a file read only through a computed path, such as `#include name + ".typ"`, is not checked for changes.

## Styling

Colours and other settings are in the `redline-style` dictionary at the top of [src/preamble.typ](src/preamble.typ). Copy that file, edit it, and pass it with `--preamble my-style.typ`; the six colours of `--each-commit` and `--steps` are in `redline-style.steps`. The data the tool passes to the preamble changes between versions, so copy it again after each update.

## VS Code

The extension in [`vscode/`](vscode) runs `typst-redline` from VS Code: open a `.typ` file, click the compare button in the editor's title bar or right-click the file in the Explorer, pick what to compare with, and the PDF with the changes marked opens. It needs `typst-redline` installed as above and is not on the Marketplace; build it with Node.js and install it by hand:

```
cd vscode
npm ci
npm run package     # makes typst-redline-0.1.0.vsix
code --install-extension typst-redline-0.1.0.vsix
```

or use *Extensions: Install from VSIX...* in VS Code. [vscode/README.md](vscode/README.md) describes the lists, views, commands and settings.

## Claude Code

[`claude-plugin/`](claude-plugin) is a plugin with a skill that lets Claude Code compile a Typst document and make its redline, for requests like *"send me the notes and what changed since Monday"*. Install it with

```
/plugin marketplace add cvsvensson/typst-redline
/plugin install typst-redline@typst-redline
```

## See also

Other tools that compare Typst documents: [typdiff](https://github.com/sou1118/typdiff) and [typst-diff](https://github.com/alexanderkoller/typst-diff).

## License

MIT
