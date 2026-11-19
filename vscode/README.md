# Typst Redline

Shows what changed in a Typst document as a PDF of the new version with the changes marked. The extension runs the [`typst-redline`](https://github.com/cvsvensson/typst-redline) command-line tool, which needs `typst` and, for comparisons with commits, git. Install `typst-redline` first and restart VS Code so that it is on the `PATH`, or set `typstRedline.executable` to its full path.

The extension is not on the Marketplace. The release workflow attaches one `.vsix` per platform (win32-x64, darwin-x64, darwin-arm64, linux-x64, linux-arm64) to the release page, with `typst-redline` inside, so no `PATH` setup is needed; install it with *Extensions: Install from VSIX...*. A `typstRedline.executable` you set takes precedence. To build a `.vsix` without the binary, run `npm ci` and `npm run package` in the [`vscode/`](https://github.com/cvsvensson/typst-redline/tree/main/vscode) folder of the repository. It is turned off in Restricted Mode, because it runs git and the programs its settings name.

To see what changed:

1. Open the main `.typ` file of the document.
2. Run **Typst Redline: Show changes...** from the Command Palette, or from the right-click menu of a `.typ` file in the Explorer, the editor or its tab.
3. Pick what to show, or press Enter for the first entry of the list. The PDF opens beside the document if a PDF viewer extension is installed, otherwise in the system viewer.
4. When you have read it, run **Typst Redline: Mark as reviewed**. It marks the commit that is checked out; uncommitted changes are not part of it.

The list starts with *Last reviewed version*, the changes since the commit you last marked, once there is one; then *Uncommitted changes*, when files git tracks in the project, or the file itself if git does not track it, differ from the checked-out commit; then the commits that touch the project, newest first. **A commit stands for its own change and every change after it**, here and in every other list of commits. The first commit of the repository has no version before it and is not listed. After the commits come *Another file...*, which compares with a `.typ` file of your choice, *Several commits...* and *More options...*.

**The view.** *More options...* shows the current view, *Marked* at first, and *View* there changes it and returns to the list; the workspace keeps the view until you change it.

- *Marked* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#text-formulas-and-figures)): the new version with the changes marked.
- *Side by side* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#slides-side-by-side)): changed pages of the old and the new version next to each other. Needs Typst 0.14 or later.
- *Marked, coloured by commit* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#who-changed-what)) and *Side by side, coloured by commit*: each change in the colour of the commit that made it. A second list chooses *One colour per commit* or *2 steps* to *6 steps*, which split the commits into groups of about equal change. One colour per commit with more than six commits gives six steps. Where there is nothing to colour, as with another file, the changes are shown in the same layout without colours, and a notification says so.

**More ways to compare.**

- *Several commits...* lets you tick up to six commits: every change from the oldest ticked commit on is shown, and each ticked commit starts a new colour, which the commits after it share up to the next tick. It uses the layout of the view.
- *Between two commits...*, under *More options...*, asks for the oldest and the newest commit to show; the new version is then that commit, not the file as it is now. It uses the view, colours included.

The PDF is written next to the file as `<name>.redline.pdf`. Messages of the tool are in the *Typst Redline* output.

Settings, with their defaults:

- `typstRedline.executable`: `typst-redline`, and `typstRedline.typst`: empty, for `typst` on `PATH`.
- `typstRedline.root`: empty, for the workspace folder, or the git repository if the workspace folder contains it.
- `typstRedline.contextMenu`: `true`; `false` removes the right-click entry.
- `typstRedline.openPdf`: `auto`; `editor` or `external` choose one place.
- `typstRedline.extraArgs`: empty; further arguments, such as `["--no-moves"]` or `["--", "--font-path", "fonts"]`. The command sets `--since`, `--to`, `--layout`, `--each-commit` and `--steps` itself and refuses to run if this setting holds one of them.
