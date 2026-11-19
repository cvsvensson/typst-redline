# Typst Redline

Shows what changed in a Typst document as a PDF of the new version with the changes marked. The extension runs the [`typst-redline`](https://github.com/cvsvensson/typst-redline) command-line tool, which needs `typst` and, for comparisons with commits, git. Install `typst-redline` first and restart VS Code so that it is on the `PATH`, or set `typstRedline.executable` to its full path.

The extension is not on the Marketplace. Build the `.vsix` from the [`vscode/`](https://github.com/cvsvensson/typst-redline/tree/main/vscode) folder of the repository with `npm ci` and `npm run package`, and install it with *Extensions: Install from VSIX...*. It is turned off in Restricted Mode, because it runs git and the programs its settings name.

To see what changed:

1. Open the main `.typ` file of the document.
2. Run **Typst Redline: Show changes since...** from the Command Palette, or from the right-click menu of a `.typ` file in the Explorer, the editor or its tab.
3. Press Enter for the changes since the newest commit, or since the last reviewed version once there is one; or pick another commit or *Another file...*. The PDF opens beside the document if a PDF viewer extension is installed, otherwise in the system viewer.
4. When you have read it, run **Typst Redline: Mark as reviewed**. It marks the commit that is checked out; uncommitted changes are not part of it. Next time, *Last reviewed version* shows what changed since that commit.

**The view.** The first entry of the list, *View: Marked* at first, shows how the changes are shown. Picking it changes the view and returns to the list; the workspace keeps the view until you change it.

- *Marked* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#text-formulas-and-figures)): the new version with the changes marked.
- *Side by side* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#slides-side-by-side)): changed pages of the old and the new version next to each other. Needs Typst 0.14 or later.
- *Marked, coloured by commit* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#who-changed-what)) and *Side by side, coloured by commit*: each change in the colour of the commit that made it. A second list chooses *One colour per commit* or *2 steps* to *6 steps*, which split the commits into groups of about equal change. One colour per commit with more than six commits gives six steps. Where there is nothing to colour, as with another file, the changes are shown in the same layout without colours, and a notification says so.

**More ways to compare.** Changes *since* a commit are those made after it; a commit picked *from* or *to* has its own change included.

- *Several commits...* (with at least two commits) lets you tick up to six commits: every change from the oldest ticked commit on is shown, and each ticked commit starts a new colour, which the commits after it share up to the next tick. It uses the layout of the view.
- *Between two commits...* asks for the oldest and the newest commit to show; the new version is then that commit, not the file as it is now. It uses the view, colours included.

The PDF is written next to the file as `<name>.redline.pdf`. Messages of the tool are in the *Typst Redline* output.

Settings, with their defaults:

- `typstRedline.executable`: `typst-redline`, and `typstRedline.typst`: empty, for `typst` on `PATH`.
- `typstRedline.root`: empty, for the workspace folder, or the git repository if the workspace folder contains it.
- `typstRedline.contextMenu`: `true`; `false` removes the right-click entry.
- `typstRedline.openPdf`: `auto`; `editor` or `external` choose one place.
- `typstRedline.extraArgs`: empty; further arguments, such as `["--no-moves"]` or `["--", "--font-path", "fonts"]`. The command sets `--since`, `--to`, `--layout`, `--each-commit` and `--steps` itself and refuses to run if this setting holds one of them.
