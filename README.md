# typst-redline

`typst-redline` shows what changed between two versions of a [Typst](https://typst.app) document. It makes a PDF of the new version with deleted text struck out, new text highlighted, changed formulas and figures framed, and bars in the margin; a summary on the last page lists every change, including those it cannot mark on the page. It does for Typst what `latexdiff` does for LaTeX. AI agents wrote this tool.

<img src="docs/paper.png" width="480" alt="A paper page: a deleted word struck out in red, new words highlighted in blue, the old formula and plot in dashed red frames above the new ones in blue frames, a new paragraph in blue, and bars in the margin next to each changed line, with the number of each change beside them">

The [gallery](docs/gallery.md) shows every kind of mark, each with the files it was made from.

## Install

`typst-redline` compiles with the [`typst`](https://github.com/typst/typst#installation) on your `PATH`, and uses [git](https://git-scm.com/downloads) for comparisons with commits; comparing two files needs no git.

**Prebuilt binary.** Each release on the [Releases page](https://github.com/cvsvensson/typst-redline/releases) has an archive per platform:

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

**From source**, with [Rust](https://rustup.rs) (on Windows, Rust needs the Visual Studio C++ build tools, which the Rust installer offers to install):

```
cargo install --git https://github.com/cvsvensson/typst-redline
```

This puts `typst-redline` in `~/.cargo/bin`, which the Rust installer adds to `PATH`. Run the same command with `--force` to update, and `cargo uninstall typst-redline` to remove it.

`typst-redline --version` checks that it is found. The tests run with Typst 0.15.1 on Linux, macOS and Windows, and with Typst 0.14.2 on Linux; they also pass with Typst 0.11.1 on Windows.

## Use

In a git repository:

```
typst-redline paper.typ                    # uncommitted changes, since the last commit
typst-redline paper.typ --since HEAD~3     # changes since three commits ago
typst-redline paper.typ --since v1         # changes since a tag, branch or commit hash
typst-redline paper.typ --since v1..v2     # changes between two commits
```

Without git, compare two copies; quote paths that contain spaces:

```
typst-redline "thesis v1/thesis.typ" "thesis v2/thesis.typ"
```

The PDF is written next to the file, or next to the new file when you give two, as `<name>.redline.pdf`, and its path is printed. `-o out.pdf` writes it elsewhere. With one file, the new version is the file as it is on disk, uncommitted changes included, unless `--to <revision>` names another.

If the document reads files outside its own folder, such as `#include "../shared/intro.typ"`, pass the folder that holds all of its files with `--root`, for example `typst-redline thesis/main.typ --root .` from that folder. Without `--root`, the `TYPST_ROOT` environment variable is used if it is set, and otherwise the folder of the file. `--typst <path>` picks another `typst` than the one on `PATH`, and arguments after `--` go to `typst compile`, for example `-- --font-path fonts`. `--keep-typ` keeps the marked Typst source and prints where it is. `typst-redline --help` lists every option.

## What is marked

Each item links to a picture in the gallery.

- **Text** ([picture](docs/gallery.md#text-formulas-and-figures)): deleted words are struck out in red, new words highlighted in blue, and a wholly new paragraph is blue. This also works inside headings, list items, captions, bold and emphasized text. A deleted list item stays in its list, with its old marker or number in red, and a deleted line break shows as a struck ↵ instead of breaking the line.
- **Formulas, figures, code blocks and drawings** ([picture](docs/gallery.md#text-formulas-and-figures)) are shown whole: the old one in a dashed red frame, crossed out where it cannot be struck through, the new one in a blue frame. A drawing in a box in the text, such as an icon, gets a frame of its own size. Code blocks and filled boxes keep their colours inside the frame. Where the text is light, as on a dark slide, deleted words get a light background and new paragraphs a lighter blue.
- **Bars in the margin** point to every changed line: dashed for deletions, solid for insertions.
- **Numbers**: next to the bars, each change has a small grey number, counted in the reading order of the new version, so that a co-author can write "change 14 is wrong". Changes that start on one line share a label such as *3–5*. The summary page uses the same numbers, also for changes that have no mark on the pages.
- **Moved text** ([picture](docs/gallery.md#moved-text)): a paragraph, heading, list item or section moved elsewhere, also into another file, is shown once, unmarked, at its new place, with a grey bracket and a "moved" note in the margin. At its old place, a grey line with an arrow names its first words and links to it. Both places show the move's number. Edits made while moving are marked as usual. `--no-moves` shows moved text as deleted at one place and inserted at the other.
- **Several files** ([picture](docs/gallery.md#several-files-and-git)): changes in files the document includes or imports are marked where they appear. With git, the old version of each file is taken from the old commit.
- **Replaced images and data files** ([picture](docs/gallery.md#several-files-and-git)): when an image changes but the `image("...")` call stays the same, the figure is marked like any changed figure, old image above new. A changed data file read in code that does not show on the page, such as `#let data = json("data.json")`, is listed on the summary page with its name.
- **Template arguments and code** ([picture](docs/gallery.md#the-summary-page)): a change inside a content argument in brackets, such as a template's `abstract: [...]`, is marked like other text. A changed string or other value, such as `title: "..."` or a colour, and changed code that does not show on the page, such as a `#set` rule or a function, are listed on the summary page in words, for example `title: "Sorting on small devices" → "Sorting in 32 kB"`. Another call whose arguments change is marked whole, old call deleted and new one inserted.
- **Citations and bibliography**: a new or removed citation is marked in the text; bibliography entries that appear, disappear or change are listed on the summary page.
- **Changes that do not reach the page**: a change in content that is never shown, or inside `hide`, or in text that a function reads as a string, cannot carry a mark; the summary page lists it with its source text.
- **Who changed what** ([picture](docs/gallery.md#who-changed-what)): with `--each-commit`, each change has the colour of the commit that made it.

With the deleted parts removed, the marked PDF has the same text, layout and numbering as the new version (see Known problems for the exceptions). Deleted figures and equations keep their old numbers without shifting the numbers after them, and references in deleted text point to the right place in this PDF.

## The summary page

The last page lists what changed, so that nothing is missed when a change cannot be marked. A line at the top of the first page says where the summary is and how many changes it lists that are not marked on the pages.

<img src="docs/report-summary.png" width="480" alt="The summary page: a sentence with the number of changes, a table of changed sections with the numbers of their changes, counts and pages, a changed title shown as old and new text, a new bibliography entry, the commits in the range, and the legend">

From the top:

- One sentence counts the changes and how many of them are not marked on the pages. When the versions do not differ, it says *No changes found between the two versions*, the line on the first page says so too, and so does a note on stderr.
- **Changes by section**: each section with changes, the numbers of its changes, how many changes it has (not a page count), how many passages moved, and the page where the section starts. Changes before the first heading, such as in `#set` rules or a template call, are under *Start of the document*. Page numbers are the ones printed on the page; a page without a printed number is given as its PDF page.
- **Marked with a bar only**: changes that have a bar in the margin but nothing else to show, such as a blank line added before a list item, which spaces it apart; each is explained in words, after its number.
- **Not marked in place**: each change that has no mark on the pages, after its number, said in words with old in red and new in blue, above its source in grey. Bibliography entries that changed are listed here too.
- **What this PDF shows differently**: notes on where this PDF differs from a plain proof, for example pages without bars or coarser marks; see Known problems.
- **Commits**: for a comparison with git, the commits between the two versions that change the document or a file it reads.
- **How changes are marked**: the legend.

The summary uses the page size and margins of the document's leading `#set page` rule, but not its template, header or footer.

## Slides and side by side

`--layout side-by-side` shows each changed page of the old version next to the same page of the new version and leaves out pages without changes. The first page lists which pages are shown. It needs Typst 0.14 or later.

```
typst-redline talk.typ --since HEAD~1 --layout side-by-side
```

<img src="docs/slides.png" width="800" alt="An old slide with a deleted list item and an old bar chart in a dashed frame, next to the new slide with the new list item and the new chart in a blue frame">

This is the layout for slides. In the default layout, deleted content is shown on the same page as the new content, so a full slide overflows; side by side, each page holds only its own version. A page that moved is shown next to its old page. Deleted content looks as it did in the old version, since the old pages are shown as they were.

## Recipes with git

**Send a co-author what changed since the version they read.** Tag the commit you sent, `git tag sent-to-anna`, and later run `typst-redline paper.typ --since sent-to-anna`. The summary lists the commits in between.

**What changed since you last read it.** When you have read the document, run `typst-redline paper.typ --mark-reviewed`; later, `typst-redline paper.typ --since reviewed` shows what changed since. The mark is the last commit, kept in a local git ref per document, printed when you set it; `git push` does not send it, and `git update-ref -d <that ref>` removes it. If the repository has a branch or tag named `reviewed`, `--since reviewed` means that instead and says so on stderr.

**What changed since a date.** Find the last commit before it and compare with that: `typst-redline paper.typ --since $(git rev-list -1 --before="2026-11-16 00:00" HEAD)`. Give a date, not a weekday name, which git reads loosely; if no commit is that old, the command prints nothing and `--since` reports a missing value.

**What a branch changed**: `typst-redline paper.typ --since main...feature` compares the point where `feature` left `main` with `feature`.

**Who changed what.** `--each-commit` gives each commit between the two versions its own colour, for up to 6 commits that change the document or a file it reads. The commits are listed in their colours on the summary page and in one small line in the bottom margin of each marked page.

```
typst-redline paper.typ --since sent-to-anna --each-commit
```

For more commits, `--steps` groups them into up to 6 steps, each in its own colour: `--steps 3` splits them into three steps that change about the same amount, and `--steps a1b2c3,submitted` ends steps at those revisions, the last step at the new version, uncommitted changes included. The summary lists each step with its range, date and commit subjects.

Only the first and the last version are compared, so text that one commit added and a later one removed does not appear, and a passage that several commits changed takes the colour of the commit that changed most of it. Both options need the old revision to be an ancestor of the new one, and do not work with two plain files.

## Known problems

Each of these was tried with the current version.

- Some moved text shows as deleted at one place and inserted at the other: a sentence moved within or between paragraphs (text after a display equation belongs to the paragraph of the equation), paragraphs reordered inside a moved section, a nested list item moved on its own, a passage of fewer than four words unless it moves with a longer one next to it, text inside a call's content argument such as `#slide[...]`, text moved into or out of a file that is not shown in order (imported, included by code, or no longer included), a passage rewritten so much that it keeps less than about 60% of its words, and moved setup code such as `#set` and `#let`.
- Deleted content shifts line and page breaks compared with the new version. A deleted figure, table or drawing that fits on a page is kept whole, so on a full slide it and the new one each take a page of their own.
- A reference or citation in deleted text to a label or bibliography entry that the new version lacks shows the label or key, not the number the old version printed. A deleted citation of an entry that the new version keeps shows that entry's number in the new bibliography.
- Deleted content is laid out under the new version's show and set rules, so where such a rule changed, deleted content does not look as it did in the old version. Side by side shows the old pages as they were.
- Deleted content that updates a state, for example in a show rule of a package, leaves the state changed after it, because Typst records no initial value to set it back to; the summary page names such states. Counters are set back.
- Deleted figures, equations and citations are still figures, equations and citations, so the document's own queries for them, such as a count with `query(figure)`, also find the deleted ones. `outline` leaves them out.
- A citation added to or removed from a group such as `@a @b` marks the whole group.
- Where a function reads its content as text, a changed text can be missing from the page, not only its mark; the summary page lists it. A function that fails on marked content makes the whole document fall back to coarser marks, and the summary says so.
- A changed element that sets the page from inside a function, such as a slide, cannot be framed. The document is then compiled again without frames around content computed at layout (calls of functions built on `context` or `layout`, which includes many drawings), and the summary says so.
- Pages where a template or a later `set page` rule gives both its own page foreground and background get no margin bars; the summary lists those pages.
- In deleted code, a label passed as a named argument or written in math is taken to be attached by the code and is renamed; a label passed positionally is taken to be a reference. Code that uses labels the other way makes the tool show deleted code by name.
- Show rules on `strike`, `highlight` or `text` in your document also restyle the marks. A text show rule with a `regex` selector stops matching where a mark cuts its match; for a string selector, such as `#show "Typst": smallcaps`, the mark is widened to cover the match.
- A box or table deleted or inserted whole whose fill comes from a `set` or `show` rule has its text coloured like other text, which can be hard to read on a dark fill.
- A moved passage in a middle column of a page with three or more columns, and one in a narrow margin, has a bracket but no "moved" note. Changes in such a middle column have no number in the margin.
- Side by side shows the numbers of changes on the new pages only, a deletion's number where the deleted content was; its summary lists changes without numbers.
- In side by side, an inserted or deleted page is shown next to an empty one only when the pages have headings or a moved passage on them; otherwise it can be paired with a neighbouring page.
- Files outside the project root and package files are not checked for changes.
- Typst 0.12 and 0.13 are not tested. With Typst 0.11 to 0.13, side by side does not work, and a changed file that the document reads only through a computed path, such as `#include name + ".typ"`, is neither marked nor listed; the summary says that such files were not checked. With Typst 0.11, the summary keeps the document's header and footer, right-to-left text gets no bars, and a changed drawing that needs a finite width causes the coarser marks described above.

## Styling

Colours and other settings are in the `redline-style` dictionary at the top of [src/preamble.typ](src/preamble.typ). Copy that file, edit it, and pass it with `--preamble my-style.typ`; the six colours of `--each-commit` and `--steps` are in `redline-style.steps`. The tool passes data to the functions in the preamble, and that data changes between versions, so copy the preamble again from the version you run after each update.

## VS Code

The extension in [`vscode/`](vscode) runs `typst-redline` on the open `.typ` file. It needs `typst-redline` installed as above and is not on the Marketplace; build it with Node.js and install it by hand:

```
cd vscode
npm ci
npm run package     # makes typst-redline-0.1.0.vsix
code --install-extension typst-redline-0.1.0.vsix
```

or use *Extensions: Install from VSIX…* in VS Code. VS Code finds `typst-redline` on the `PATH` it started with, so restart it after installing the tool. The extension is turned off in Restricted Mode, because it runs git and the programs its settings name. It adds two commands:

- *Typst Redline: Show changes since…* lists the recent commits that changed the project, the last reviewed version and *Another file…*; picking one makes the PDF and opens it. Run it on the main file of a multi-file document. *Options…* at the end of the list sets side by side, colour by commit, colour by step and no moves for this and later runs. It saves the open files of the project first; a progress notification can cancel the run. Warnings of the tool show as a notification, and everything it prints is in the *Typst Redline* output.
- *Typst Redline: Mark as reviewed* runs `--mark-reviewed` for the open file.

The settings are `typstRedline.executable` and `typstRedline.typst` for the paths of the two programs, `typstRedline.root` for `--root`, `typstRedline.extraArgs` for further arguments, such as `["--", "--font-path", "fonts"]`, and `typstRedline.openPdf`. The root is by default the workspace folder, or the git repository if the workspace folder contains it. The PDF opens beside the document when a PDF viewer extension is installed, otherwise in the system viewer.

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
