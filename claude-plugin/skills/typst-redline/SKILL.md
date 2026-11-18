---
name: typst-redline
description: Compile a Typst document to PDF and make a PDF marking what changed in it since a git revision or between two versions, then give both to the user. Use when the user asks for their Typst notes, paper or document as a PDF, asks what changed in it, or asks for a diff, redline or changes since some point.
---

# Typst PDFs and redlines

## Find the document

The document is the `.typ` file the user names. Otherwise, take the one that is not `#include`d or `#import`ed by another `.typ` file in the project. If several remain, ask which.

## Make the PDFs

- The current version: `typst compile <file.typ>` writes `<file>.pdf`.
- The changes: `typst-redline <file.typ> [--since <rev>]` writes `<file>.redline.pdf` and prints its path on stdout. Without `--since` it shows the uncommitted changes.
  - The user names a time ("since yesterday", "since Monday"): `--since $(git rev-list -1 --before="<time>" HEAD)`.
  - A commit, tag or branch: `--since <rev>`; between two: `--since <a> --to <b>`; since a branch left main: `--since main...HEAD`.
  - Two files, or a file at a revision: `typst-redline <old> <new>`, each a path or `REV:PATH`.
  - Who changed what: `--each-commit` colours each change by its commit (at most 6); for more commits, `--steps <n>` or `--steps <rev>,<rev>` colours them in groups.
  - What changed since the user last reviewed it: `--since reviewed`; once they have read it, `typst-redline <file.typ> --mark-reviewed` records the last commit as reviewed.
- `typst-redline --help` lists the rest.

When the user only asks for the changes, the redline alone is enough: it is the new version with changes marked.

Diagnostics go to stderr. Pass on to the user any note that changes were *not marked* or that a fallback was used. If the first page lists no changes, say so and don't send an empty redline.

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

Other platforms use the same release pages with their target name (`aarch64-apple-darwin`, `x86_64-pc-windows-msvc` as `.zip`, ...). If no release fits, `cargo install --git https://github.com/cvsvensson/typst-redline`.
