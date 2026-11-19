# Typst Redline

Shows what changed in a Typst document as a PDF of the new version with the changes marked. The extension runs the [`typst-redline`](https://github.com/cvsvensson/typst-redline) command-line tool, which needs `typst` and, for comparisons with commits, git. Install `typst-redline` first and restart VS Code so that it is on the `PATH`, or set `typstRedline.executable` to its full path.

To see what changed:

1. Open the main `.typ` file of the document.
2. Run **Typst Redline: Show changes since...** from the Command Palette.
3. Pick what to compare with: *Last reviewed version* (once there is one), a commit, *Several commits...* or *Another file...*.
4. Pick the view. The one used last is at the top, so Enter repeats it.
5. The PDF opens beside the document if a PDF viewer extension is installed, otherwise in the system viewer.
6. When you have read it, run **Typst Redline: Mark as reviewed**. Next time, *Last reviewed version* compares with the commit you read.

The views:

- *Marked PDF* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#text-formulas-and-figures)): the new version with the changes marked.
- *Side by side* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#slides-side-by-side)): changed pages of the old and the new version next to each other. Needs Typst 0.14 or later.
- *Coloured by commit* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#who-changed-what)) and *Side by side, coloured by commit*: each change in the colour of the commit that made it; with more than six commits, six colours for groups of commits. They are offered when there are at least two commits, or a commit and uncommitted changes, to colour.
- *Several commits...* lets you tick commits: the changes since the oldest ticked one are shown, in a new colour after each ticked commit.

The PDF is written next to the file as `<name>.redline.pdf`. Messages of the tool are in the *Typst Redline* output.

Settings, with their defaults:

- `typstRedline.view`: `ask`. Set `marked`, `side-by-side`, `by-commit` or `side-by-side-by-commit` to skip step 4.
- `typstRedline.executable`: `typst-redline`, and `typstRedline.typst`: empty, for `typst` on `PATH`.
- `typstRedline.root`: empty, for the workspace folder, or the git repository if the workspace folder contains it.
- `typstRedline.openPdf`: `auto`; `editor` or `external` choose one place.
- `typstRedline.extraArgs`: empty; further arguments, such as `["--no-moves"]` or `["--", "--font-path", "fonts"]`.
