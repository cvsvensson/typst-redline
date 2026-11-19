---
name: typst-redline
description: Compile a Typst document to PDF and make a PDF marking what changed in it since a git revision or between two versions, then give both to the user. Use when the user asks for their Typst notes, paper or document as a PDF, asks what changed in it, or asks for a diff, redline or changes since some point.
---

# Typst PDFs and redlines

## Find the document

The document is the `.typ` file the user names. Otherwise, take the one that is not `#include`d or `#import`ed by another `.typ` file in the project. If several remain, ask which.

## Make the PDFs

- The current version: `typst compile <file.typ>` writes `<file>.pdf`.
- The changes: `typst-redline <file.typ> --since <rev>` writes `<file>.redline.pdf` next to the file and prints its path on stdout. The new version is the file on disk, uncommitted changes included; without `--since`, the old version is the last commit. `-o <path>` writes the PDF elsewhere, for example outside the user's repository.
  - A commit, tag or branch: `--since <rev>`; between two: `--since <a> --to <b>`; since a branch left main: `--since main...HEAD`.
  - A time ("since Monday", "since last week"): work out the calendar date from today's date, then find the last commit before it: `git log -1 --format='%h %ad %s' --date=short --before='<YYYY-MM-DD> 00:00'`. Do not pass a weekday name to git: it reads it loosely and can return the latest commit. If a commit is printed, use its hash with `--since` and tell the user which commit (date and subject) you compared with. If nothing is printed, no commit is that old: compare with the first commit, `git rev-list --max-parents=0 HEAD`, and tell the user that what the first commit contained is not marked.
  - Two files, or a file at a revision: `typst-redline <old> <new>`, each a path or `REV:PATH`. Quote paths with spaces. The PDF goes next to the new file.
  - Files outside the document's folder (`#include "../x.typ"`): add `--root <folder holding all of them>`.
  - Who changed what: `--each-commit` colours each change by its commit (at most 6); for more commits, `--steps <n>` or `--steps <rev>,<rev>` colours them in groups.
  - Slides: add `--layout side-by-side` (needs Typst 0.14 or later).
  - What changed since the user last reviewed it: `--since reviewed`; once they have read it, `typst-redline <file.typ> --mark-reviewed` records the last commit as reviewed.
- `typst-redline --help` lists the rest.

When the user only asks for the changes, the redline alone is enough: it is the new version with changes marked.

## Read the result before you report it

Diagnostics go to stderr; pass on any warning or note. Then read the summary at the end of the PDF; the line at the top of the first page gives its page (`pdftotext` or reading the PDF works):

- *No changes found between the two versions*: say so and do not send the PDF as if it showed changes. Check that you compared with the right revision.
- *Changes by section*: *No.* gives the numbers of the section's changes, which also stand next to their marks in the margin; the count after it counts changes, not pages. Refer to changes by these numbers when you report them.
- *Not marked in place*: changes with no mark on the pages, such as a changed title string, a `#set` rule, a function body or a replaced data file. A `#set` rule or function can change the whole document. Tell the user about each in words; they are easy to miss in the PDF.
- *What this PDF shows differently*: notes such as coarser marks or pages without bars. Mention them.

Report what the summary says, not more: do not describe changes you did not see in it or on the pages.

## Give them to the user

If the session has a tool for sending files to the user, send the PDFs with it. Otherwise, give their paths.

## If a tool is missing

If `typst` or `typst-redline` is not on `PATH`, install the release binary for this platform into `~/.local/bin` (and add that to `PATH` for the session). On Linux x86_64:

```sh
mkdir -p ~/.local/bin
curl -sSL https://github.com/typst/typst/releases/latest/download/typst-x86_64-unknown-linux-musl.tar.xz \
  | tar -xJ --strip-components=1 -C ~/.local/bin typst-x86_64-unknown-linux-musl/typst
curl -sSL https://github.com/cvsvensson/typst-redline/releases/latest/download/typst-redline-x86_64-unknown-linux-musl.tar.gz \
  | tar -xz -C ~/.local/bin typst-redline
```

Other platforms use the same release page with their target name: `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, and `x86_64-pc-windows-msvc` as a `.zip`. If no release fits, `cargo install --git https://github.com/cvsvensson/typst-redline`.
