# typst-redline

`typst-redline` shows what changed between two versions of a [Typst](https://typst.app) document. It makes a PDF of the new version with deleted text struck out, new text highlighted, changed formulas and figures framed, and bars and change numbers in the margin; a summary at the end lists every change, including those it cannot mark on the page. It does for Typst what `latexdiff` does for LaTeX. AI agents wrote this tool.

<img src="docs/paper.png" width="480" alt="A paper page: a deleted word struck out in red, new words highlighted in blue, the old formula tinted and the old plot crossed out in dashed red frames above the new ones in blue frames, a new paragraph in blue, and bars in the margin next to each changed line, with the number of each change beside them">

The [gallery](docs/gallery.md) shows every kind of mark, each with the files it was made from.

## Install

`typst-redline` compiles with the [`typst`](https://github.com/typst/typst#installation) on your `PATH`, and uses [git](https://git-scm.com/downloads) for comparisons with commits; comparing two files needs no git.

**From source.** No release has been published yet, so install it with [Rust](https://rustup.rs) (on Windows, Rust needs the Visual Studio C++ build tools, which the Rust installer offers to install):

```
cargo install --git https://github.com/cvsvensson/typst-redline
```

This puts `typst-redline` in `~/.cargo/bin`, which the Rust installer adds to `PATH`. Run the same command with `--force` to update, and `cargo uninstall typst-redline` to remove it.

**Prebuilt binary.** The release workflow attaches an archive per platform to each release on the [Releases page](https://github.com/cvsvensson/typst-redline/releases):

| Platform | Archive |
|---|---|
| Linux x86_64 | `typst-redline-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `typst-redline-aarch64-unknown-linux-musl.tar.gz` |
| macOS Apple silicon | `typst-redline-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `typst-redline-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `typst-redline-x86_64-pc-windows-msvc.zip` |

Unpack it and put `typst-redline` (`typst-redline.exe` on Windows) in a folder on your `PATH`, such as `~/.local/bin`. The Linux binaries are statically linked and run on any distribution. On Linux and macOS one command does it, here for Apple silicon:

```sh
curl -sSL https://github.com/cvsvensson/typst-redline/releases/latest/download/typst-redline-aarch64-apple-darwin.tar.gz | tar -xz -C ~/.local/bin typst-redline
```

The macOS binaries are not notarized: macOS refuses to run one downloaded with a browser until you run `xattr -d com.apple.quarantine ~/.local/bin/typst-redline`. Files downloaded with `curl` are not affected.

`typst-redline --version` checks that it is found. The tests run with Typst 0.15.1 on Linux, macOS and Windows, and with Typst 0.14.2 on Linux; they also pass with Typst 0.11.1 on Windows.

## Use

In a git repository:

```
typst-redline paper.typ                    # uncommitted changes, since the last commit
typst-redline paper.typ --since HEAD~3     # changes since three commits ago
typst-redline paper.typ --since v1         # changes since a tag, branch or commit hash
typst-redline paper.typ --since v1..v2     # changes between two commits
```

Run `typst-redline` without a file in a terminal and it asks which file and which version to compare with, then prints the command it runs, such as `typst-redline paper.typ --since a82c764`. Files that another file in the folder includes come last, and with one document there is no file question. The versions are the last reviewed version, if there is one, and the newest commits:

```
Show the changes to paper.typ made after:
   1  the last reviewed version
   2  5c09e25  2026-11-19  Revise the body
   3  a82c764  2026-11-19  Expand the introduction
Number [1]:
```

Enter takes the first entry and `q` quits. Other options, such as `--layout side-by-side`, apply as given.

Without git, compare two copies; quote paths that contain spaces:

```
typst-redline "thesis v1/thesis.typ" "thesis v2/thesis.typ"
```

The PDF is written next to the file, or next to the new file when you give two, as `<name>.redline.pdf`, and its path is printed. `-o out.pdf` writes it elsewhere. With one file, the new version is the file as it is on disk, uncommitted changes included, unless `--to <revision>` names another.

If the document reads files outside its own folder, such as `#include "../shared/intro.typ"`, pass the folder that holds all of its files with `--root`, for example `typst-redline thesis/main.typ --root .` from that folder. Without `--root`, the `TYPST_ROOT` environment variable is used if it is set, and otherwise the folder of the file. `--typst <path>` picks another `typst` than the one on `PATH`, and arguments after `--` go to `typst compile`, for example `-- --font-path fonts`. `--keep-typ` keeps the marked Typst source and prints where it is. `typst-redline --help` lists every option.

## What is marked

Each item links to a picture in the gallery.

- **Text** ([picture](docs/gallery.md#text-formulas-and-figures)): deleted words are struck out in red, new words highlighted in blue, and a wholly new paragraph is blue. This also works inside headings, list items, captions, bold and emphasized text. A deleted list item stays in its list, with its old marker or number in red, and a deleted line break shows as a struck ↵ instead of breaking the line.
- **Footnotes** ([picture](docs/gallery.md#footnotes)): a new footnote is highlighted at the foot of the page; a deleted one is struck through in parentheses where it stood, so that the footnotes keep the new version's numbers.
- **Formulas, figures, code blocks and drawings** ([picture](docs/gallery.md#text-formulas-and-figures)) are shown whole: the old one in a dashed red frame, the new one in a blue frame. In the old one, text is struck through where it can be; an image or a drawing, such as a plot, is crossed out, and anything else, such as a formula, a table, a code block or a chart built from boxes, is tinted red. A drawing in a box in the text, such as an icon, gets a frame of its own size. Code blocks and filled boxes keep their colours inside the frame. Where the text is light, as on a dark slide, deleted words get a light background and new paragraphs a lighter blue.
- **Bars in the margin** point to every changed line: dashed for deletions, solid for insertions.
- **Numbers**: next to the bars, each change has a small grey number, counted in the reading order of the new version, so that a co-author can write "change 14 is wrong". Changes that start on one line share a label such as *3–5*. A change in a column between others, or next to a margin too narrow for its number, has its number in the other margin on the same line. The summary page uses the same numbers, also for changes that have no mark on the pages.
- **Moved text** ([picture](docs/gallery.md#moved-text)): a paragraph, heading, list item or section moved elsewhere, also into another file, is shown once, unmarked, at its new place, with a grey bracket and a "moved" note in the margin. At its old place, a grey line with an arrow names its first words and links to it. Both places show the move's number. Edits made while moving are marked as usual. `--no-moves` shows moved text as deleted at one place and inserted at the other.
- **Several files** ([picture](docs/gallery.md#several-files-and-git)): changes in files the document includes or imports are marked where they appear. With git, the old version of each file is taken from the old commit.
- **Replaced images and data files** ([picture](docs/gallery.md#several-files-and-git)): when an image changes but the `image("...")` call stays the same, the figure is marked like any changed figure, old image above new. A changed data file read in code that does not show on the page, such as `#let data = json("data.json")`, is listed on the summary page with its name.
- **Template arguments and code** ([picture](docs/gallery.md#the-summary-page)): a change inside a content argument in brackets, such as a template's `abstract: [...]`, is marked like other text. A changed string or other value, such as `title: "..."` or a colour, and changed code that does not show on the page, such as a `#set` rule or a function, are listed on the summary page in words, for example `title: "Sorting on small devices" → "Sorting in 32 kB"`. Another call whose arguments change is marked whole, old call deleted and new one inserted. When only the body of a call that also takes content by name changed, such as the image of a `#figure(image(...), caption: [...])`, the old body is shown deleted and the new figure inserted, so that its caption shows once, with edits in it marked. A changed call that shows the same as before, and a changed label, are listed.
- **Citations and bibliography**: a new or removed citation is marked in the text; bibliography entries that appear, disappear or change are listed on the summary page.
- **Changes that do not reach the page**: a change in content that is never shown, or inside `hide`, or in text that a function reads as a string, cannot carry a mark; the summary page lists it with its source text.
- **Who changed what** ([picture](docs/gallery.md#who-changed-what)): with `--each-commit`, each change has the colour of the commit that made it.

With the deleted parts removed, the marked PDF has the same text, layout and numbering as the new version ([Known problems](docs/known-problems.md#deleted-content-in-the-new-layout) lists the exceptions). Deleted figures and equations keep their old numbers without shifting the numbers after them, and references in deleted text point to the right place in this PDF. The summary's pages do not count in the document's page total, so a footer such as *page 3 of 12* reads as in the new version.

## The summary page

The last page, or pages, list what changed, so that nothing is missed when a change cannot be marked. A line at the top of the first page says where the summary is and how many changes it lists that are not marked on the pages.

<img src="docs/report-summary.png" width="480" alt="The summary page: a sentence with the number of changes, a table of changed sections with the numbers of their changes, counts and pages, a changed title shown as old and new text, a new bibliography entry, the commits in the range, and the legend">

From the top:

- One sentence counts the changes and how many of them are not marked on the pages. When the versions do not differ, it says *No changes found between the two versions*, the line on the first page says so too, and so does a note on stderr.
- **Changes by section**: each section with changes, the numbers of its changes, how many changes it has (not a page count; text deleted and inserted at one place is one change), how many passages moved into it, and the pages of its changes, or for a deleted section where it was. Changes before the first heading, such as in `#set` rules or a template call, are under *Start of the document*, and a change in a page header or footer is under the section whose code sets it. A heading written as a call, such as `#heading[...]`, or inside a call starts a section too. Pages are numbered as in the PDF; the column is headed *PDF page* when the document prints other page numbers.
- **Marked with a bar only**: changes that have a bar in the margin but nothing else to show, such as a blank line added before a list item, which spaces it apart, explained in words, or deleted code that draws nothing, such as a counter update, with its source; each after its number.
- **Not marked in place**: each change that has no mark on the pages, after its number, said in words with old in red and new in blue where it can be (a changed argument, a label, a definition or a rule), and its source. Bibliography entries that changed are listed here too.
- **What this PDF shows differently**: notes on where this PDF differs from a plain proof, for example pages without bars or coarser marks; see [Known problems](docs/known-problems.md).
- **Commits**: for a comparison with git, the commits between the two versions that change the document or a file it reads.
- **How changes are marked**: the legend, a sample of each kind of mark.

The summary is set in the document's font, without its template, header, footer, foreground or background, on pages as wide as the document's, or wider where that leaves less than about 25 em for text. A page is as high as the document's or as a portrait sheet of that width, whichever is higher; a summary up to half as long again takes one taller page, a longer one continues on further pages.

## Slides and side by side

`--layout side-by-side` shows each changed page of the old version next to the same page of the new version and leaves out pages without changes. The first page lists which pages are shown, and the last pages are the same summary as in the default layout, without its legend. Change numbers are on the new pages only. A page that a change only listed on the summary draws differently, such as one with renumbered headings, is shown as well, also where such a page may only have reflowed after a marked change. It needs Typst 0.14 or later and an old version that compiles; the default layout falls back to coarser marks instead.

```
typst-redline talk.typ --since HEAD~1 --layout side-by-side
```

<img src="docs/slides.png" width="800" alt="An old slide with a deleted list item and the old bar chart tinted in a dashed frame, next to the new slide with the new list item and the new chart in a blue frame">

This is the layout for slides. In the default layout, deleted content is shown on the same page as the new content, so a full slide overflows; side by side, each page holds only its own version. A page that moved is shown next to its old page. Deleted content looks as it did in the old version, since the old pages are shown as they were.

## Recipes with git

**Send a co-author what changed since the version they read.** Tag the commit you sent, `git tag sent-to-anna`, and later run `typst-redline paper.typ --since sent-to-anna`. The summary lists the commits in between.

**What changed since you last read it.** When you have read the document, run `typst-redline paper.typ --mark-reviewed`; later, `typst-redline paper.typ --since reviewed` shows what changed since. The mark is the last commit, kept in a local git ref per document, printed when you set it; `git push` does not send it, and `git update-ref -d <that ref>` removes it. If the repository has a branch or tag named `reviewed`, `--since reviewed` means that instead and says so on stderr. `typst-redline paper.typ --show-reviewed` prints the hash of the commit `--since reviewed` uses, or nothing if there is none, and `--list-files` prints the paths of the files the document is made of, one per line.

**What changed since a date.** Find the last commit before it and compare with that: `typst-redline paper.typ --since $(git rev-list -1 --before="2026-11-16 00:00" HEAD)`. Give a date, not a weekday name, which git reads loosely; if no commit is that old, the command prints nothing and `--since` reports a missing value.

**What a branch changed**: `typst-redline paper.typ --since main...feature` compares the point where `feature` left `main` with `feature`.

**Who changed what.** `--each-commit` gives each commit between the two versions its own colour, for up to 6 commits that change the document or a file it reads. The commits are listed in their colours on the summary page and in one small line in the bottom margin of each marked page.

```
typst-redline paper.typ --since sent-to-anna --each-commit
```

For more commits, `--steps` groups them into up to 6 steps, each in its own colour: `--steps 3` splits them into three steps that change about the same amount, and `--steps a1b2c3,submitted` ends steps at those revisions, the last step at the new version, uncommitted changes included. The summary lists each step with its range, date and commit subjects.

Only the first and the last version are compared, so text that one commit added and a later one removed does not appear, and a passage that several commits changed takes the colour of the commit that changed most of it. Both options need the old revision to be an ancestor of the new one, and do not work with two plain files.

## Known problems

The ones you are most likely to meet; [docs/known-problems.md](docs/known-problems.md) has the full list, each tried with the current version.

- In the default layout, deleted content takes room on the page, so line and page breaks shift, and on a full slide the old and the new version of a figure each take a page of their own; use side by side for slides.
- Some moved text shows as deleted at one place and inserted at the other, for example a sentence moved within or between paragraphs, or a passage rewritten so much that it keeps less than about 60% of its words.
- Where a function reads its content as text, a changed text can be missing from the page, not only its mark, and a function that fails on marked content makes the whole document fall back to coarser marks; the summary page says so in both cases.
- Deleted code that uses a name which meant something else at its place, such as a loop variable or a function parameter, is shown by that name, such as *[unit]*, not as it looked.
- Deleted content is laid out under the new version's show and set rules, and a reference in it to a label that the new version lacks shows the label, not the number the old version printed.
- Typst 0.12 and 0.13 are not tested; before Typst 0.14, side by side does not work and a file read only through a computed path, such as `#include name + ".typ"`, is not checked for changes.

## Styling

Colours and other settings are in the `redline-style` dictionary at the top of [src/preamble.typ](src/preamble.typ). Copy that file, edit it, and pass it with `--preamble my-style.typ`; the six colours of `--each-commit` and `--steps` are in `redline-style.steps`. The tool passes data to the functions in the preamble, and that data changes between versions, so copy the preamble again from the version you run after each update.

## VS Code

Open a `.typ` file and click the compare button in the editor's title bar to see what changed, as a PDF with the changes marked.

The extension in [`vscode/`](vscode) runs `typst-redline` on the open `.typ` file. It needs `typst-redline` installed as above and is not on the Marketplace; build it with Node.js and install it by hand:

```
cd vscode
npm ci
npm run package     # makes typst-redline-0.1.0.vsix
code --install-extension typst-redline-0.1.0.vsix
```

or use *Extensions: Install from VSIX...* in VS Code. VS Code finds `typst-redline` on the `PATH` it started with, so restart it after installing the tool. The extension is turned off in Restricted Mode, because it runs git and the programs its settings name.

To see what changed:

1. Open the main `.typ` file of the document.
2. Run *Typst Redline: Show changes...* with the compare button in the editor's title bar, from the Command Palette, or from the right-click menu of a `.typ` file in the Explorer.
3. Pick what to show, or press Enter for the first entry of the list. The PDF opens beside the document if a PDF viewer extension is installed, otherwise in the system viewer.
4. When you have read it, run *Typst Redline: Mark as reviewed*. It marks the commit that is checked out; uncommitted changes are not part of it.

Outside a git repository, or with no commits to list, the command asks for the older version's file at once; *Typst Redline: Choose view* in the Command Palette changes the view there. To compare two files, select both in the Explorer, right-click and pick *Show changes between these files*; a short list asks which one is the newer version, offering the one changed last first.

The list starts with *Last reviewed version*, the changes since the commit you last marked, once there is one; then *Changes since the last commit*, when the document's files have changes that are not committed, unsaved edits included, which are saved before the run; then the commits that change the document's files, newest first. **A commit stands for its own change and every change after it**, here and in every other list of commits. The first commit of the repository has no version before it and is not listed. After the commits come *Another file...*, which compares with a `.typ` file of your choice, *Several commits...* and *More options...*.

**The view.** *More options...* shows the current view, *Marked* at first, and *View* there changes it and returns to the list; the workspace keeps the view until you change it.

- *Marked* ([picture](docs/gallery.md#text-formulas-and-figures)): the new version with the changes marked.
- *Side by side* ([picture](docs/gallery.md#slides-side-by-side)): changed pages of the old and the new version next to each other. Needs Typst 0.14 or later.
- *Marked, coloured by commit* ([picture](docs/gallery.md#who-changed-what)) and *Side by side, coloured by commit*: each change in the colour of the commit that made it. A second list chooses *One colour per commit* or *2 steps* to *6 steps*, which split the commits into groups of about equal change. One colour per commit with more than six commits gives six steps. Where there is nothing to colour, as with another file, the changes are shown in the same layout without colours, and a notification says so.

**More ways to compare.**

- *Several commits...* lets you tick up to six commits: every change from the oldest ticked commit on is shown, and each ticked commit starts a new colour, which the commits after it share up to the next tick. It uses the layout of the view.
- *Between two commits...*, under *More options...*, asks for the oldest and the newest commit to show; the new version is then that commit, not the file as it is now. It uses the view, colours included.

If `typst-redline` or `typst` is not found, a message says which, with a button that opens install instructions and, for `typst`, one to locate it; VS Code may need a restart after installing.

The run saves the open files of the project first, and its progress notification can cancel it. The PDF is written next to the file as `<name>.redline.pdf`. Warnings of the tool show as a notification, and everything it prints is in the *Typst Redline* output.

Settings, with their defaults:

- `typstRedline.executable`: `typst-redline`, and `typstRedline.typst`: empty, for `typst` on `PATH`.
- `typstRedline.root`: empty, for the workspace folder, or the git repository if the workspace folder contains it.
- `typstRedline.contextMenu`: `true`; `false` removes the right-click entries in the Explorer.
- `typstRedline.openPdf`: `auto`; `editor` or `external` choose one place.
- `typstRedline.extraArgs`: empty; further arguments, such as `["--no-moves"]` or `["--", "--font-path", "fonts"]`. The command sets `--since`, `--to`, `--layout`, `--each-commit` and `--steps` itself and refuses to run if this setting holds one of them.

## Claude Code

[`claude-plugin/`](claude-plugin) is a plugin with a skill that lets Claude Code compile a Typst document and make its redline, for requests like *"send me the notes and what changed since Monday"*. Install it with

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
