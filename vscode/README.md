# Typst Redline

Open a `.typ` file and click the compare button in the editor's title bar to see what changed, as a PDF with the changes marked.

Shows what changed in a Typst document as a PDF of the new version with the changes marked. The extension runs the [`typst-redline`](https://github.com/cvsvensson/typst-redline) command-line tool, which needs `typst` and, for comparisons with commits, git. Install `typst-redline` first and restart VS Code so that it is on the `PATH`, or set `typstRedline.executable` to its full path.

The extension is not on the Marketplace. The release workflow attaches one `.vsix` per platform (win32-x64, darwin-x64, darwin-arm64, linux-x64, linux-arm64) to each release, with `typst-redline` inside, so no `PATH` setup is needed; no release has been published yet. Install a `.vsix` with *Extensions: Install from VSIX...*. A `typstRedline.executable` you set takes precedence. To build a `.vsix` without the binary, run `npm ci` and `npm run package` in the [`vscode/`](https://github.com/cvsvensson/typst-redline/tree/main/vscode) folder of the repository. It is turned off in Restricted Mode, because it runs git and the programs its settings name.

To see what changed:

1. Open the main `.typ` file of the document.
2. Run **Typst Redline: Show Changes...** with the compare button in the editor's title bar, from the Command Palette, or from the right-click menu of a `.typ` file in the Explorer.
3. Pick what to show, or press Enter for the first entry of the list. The PDF opens beside the document if a PDF viewer extension is installed, otherwise in the system viewer.
4. When you have read it, run **Typst Redline: Mark as Reviewed**. It marks the commit that is checked out; uncommitted changes are not part of it.

Outside a git repository, or with no commits to list, the command asks for the older version's file at once; *Typst Redline: Choose View* in the Command Palette changes the view there. To compare two files, select both in the Explorer, right-click and pick *Typst Redline: Show Changes Between These Files*; a short list asks which one is the newer version, offering the one changed last first.

The list starts with *Last reviewed version*, the changes since the commit you last marked, once there is one; then *Changes since the last commit*, when the document's files have changes that are not committed, unsaved edits included, which are saved before the run; then the commits that change the document's files, newest first. **A commit stands for its own change and every change after it**, here and in every other list of commits. The first commit of the repository has no version before it and is not listed. After the commits come *Another file...*, which compares with a `.typ` file of your choice, *Several commits...* and *Between two commits...*.

**The view.** The list's title names the current view, *Marked* at first. The button at the right of the title switches between *Marked* and *Side by side* and keeps the list open; colours stay as they are. *Typst Redline: Choose View* in the Command Palette chooses any of the four views. The workspace keeps the view until you change it.

- *Marked* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#text-formulas-and-figures)): the new version with the changes marked.
- *Side by side* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#slides-side-by-side)): changed pages of the old and the new version next to each other. Needs Typst 0.14 or later.
- *Marked, coloured by commit* ([picture](https://github.com/cvsvensson/typst-redline/blob/main/docs/gallery.md#who-changed-what)) and *Side by side, coloured by commit*: each change in the colour of the commit that made it. A second list chooses *One colour per commit* or *2 steps* to *6 steps*, which split the commits into groups of about equal change. One colour per commit with more than six commits gives six steps. Where there is nothing to colour, as with another file, the changes are shown in the same layout without colours, and a notification says so.

**More ways to compare.**

- *Several commits...* lets you tick up to six commits: every change from the oldest ticked commit on is shown, and each ticked commit starts a new colour, which the commits after it share up to the next tick. It uses the layout of the view. *Typst Redline: Show Changes for Several Commits...* in the Command Palette opens this list directly.
- *Between two commits...* asks for the oldest and the newest commit to show; the new version is then that commit, not the file as it is now. It uses the view, colours included.

If `typst-redline` or `typst` is not found, a message says which, with a button that opens install instructions and, for `typst`, one to locate it; VS Code may need a restart after installing.

The PDF is written next to the file as `<name>.redline.pdf`. Messages of the tool are in the *Typst Redline* output.

Settings, with their defaults:

- `typstRedline.executable`: `typst-redline`, and `typstRedline.typst`: empty, for `typst` on `PATH`.
- `typstRedline.root`: empty, for the workspace folder, or the git repository if the workspace folder contains it.
- `typstRedline.contextMenu`: `true`; `false` removes the right-click entries in the Explorer.
- `typstRedline.openPdf`: `auto`; `editor` or `external` choose one place.
- `typstRedline.extraArgs`: empty; further arguments, such as `["--no-moves"]` or `["--", "--font-path", "fonts"]`. The command sets `--since`, `--to`, `--layout`, `--each-commit` and `--steps` itself and refuses to run if this setting holds one of them.
