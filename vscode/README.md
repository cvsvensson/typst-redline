# Typst Redline

Shows what changed in a Typst document as a PDF of the new version with the changes marked. The extension runs the [`typst-redline`](https://github.com/cvsvensson/typst-redline) command-line tool, which needs `typst` and, for comparisons with commits, git. Install `typst-redline` first and restart VS Code so that it is on the `PATH`, or set `typstRedline.executable` to its full path.

Open the main `.typ` file of the document and run from the Command Palette:

- **Typst Redline: Show changes since…** Pick a recent commit, the last reviewed version or another file, and the PDF opens. *Options…* at the end of the list sets side by side, colour by commit, colour by step and no moves for this and later runs.
- **Typst Redline: Mark as reviewed** records the last commit as reviewed, so that *Last reviewed version* compares with it.

The PDF is written next to the file as `<name>.redline.pdf`. Messages of the tool are in the *Typst Redline* output. The settings are under *Typst Redline* in the Settings editor; see the [README](https://github.com/cvsvensson/typst-redline#vs-code) for details.
