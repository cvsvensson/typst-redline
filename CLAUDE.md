# typst-redline

A CLI that shows what changed between two versions of a Typst document, as a PDF of the new version with the changes marked. That is its whole scope: it does not compile documents itself, manage projects or replace an editor. Editors, scripts, git and CI use it by running the binary.

## What the output must satisfy

- **Every change is marked or listed.** A change the tool cannot mark in place is listed on the summary page. A silently missed change is the worst failure, worse than a coarse mark or an error.
- **The proof with the deletions removed is the new document:** same text, spacing and numbering. Marks and deleted content must not alter what they surround.
- **It reads well.** A co-author should see at a glance what changed. Judge output by rendering pages and looking at them, not only by whether it compiles.

## How the code is organized

- **The core is pure.** It maps two versions to marked-up Typst source and does not touch the filesystem, git or processes. Those live in the binary.
- **A version is a tree of files with an entry file**, backed by a directory or a git revision. Multi-file documents, old assets and git comparisons all go through this one notion.
- **Styling is separate from diffing.** The core emits calls to a few `redline-*` functions and the preamble defines how they look. A change of appearance belongs in the preamble.
- **Only the parser and the preamble depend on the Typst version.** Compilation uses the user's `typst`. Keep the preamble to features that work across the tested versions.
- **The command line is the interface.** stdout carries the result path, stderr the diagnostics; exit codes are 0 success, 1 error, 2 usage. Editor extensions stay thin wrappers around it.

## Rules for changes

- Fix the general cause. Many reported problems share a few causes; a fix that handles one document, package, editor or platform as a special case is usually the wrong fix.
- Keep the boundaries above clean, so that a later feature is a small addition. Don't add an option, type or abstraction for a need nobody has yet.
- No promises. This project is developed largely by AI agents and its author makes no commitment to maintain it. Try hard to make it work across Typst versions and platforms, but never write "supported", "guaranteed" or "we will" in the README, docs or messages. Say what is tested and what is known not to work.
- Keep `README.md` true to current behaviour, especially its Limitations section.

## Checking a change

- `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings`, `cargo test --release`. The tests call `typst` and `git` from `PATH`.
- Tests are small old/new fixtures that assert behaviour. A fixed bug gets one test built from its minimal reproduction.
- To test another Typst version, unpack its release binary into a scratch folder and put that folder first on `PATH` for the test command. Nothing needs installing.
- For a change that affects output, run the tool on a real document with git history, in a scratch clone and never in the original repository, then render pages to PNG and look at them.
- Extension: `npm run compile` and `npm test` in `vscode/`.
