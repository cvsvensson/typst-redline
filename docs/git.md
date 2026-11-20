# Recipes with git

**Send a co-author what changed since the version they read.** Tag the commit you sent, `git tag sent-to-anna`, and later run `typst-redline paper.typ --since sent-to-anna`. The summary lists the commits in between.

**What changed since you last read it.** When you have read the document, run `typst-redline paper.typ --mark-reviewed`; later, `typst-redline paper.typ --since reviewed` shows what changed since. The mark is the last commit, kept in a local git ref per document, printed when you set it; `git push` does not send it, and `git update-ref -d <that ref>` removes it. If the repository has a branch or tag named `reviewed`, `--since reviewed` means that instead and says so on stderr. `typst-redline paper.typ --show-reviewed` prints the hash of the commit `--since reviewed` uses, or nothing if there is none.

**What changed since a date.** Find the last commit before it and compare with that: `typst-redline paper.typ --since $(git rev-list -1 --before="2026-11-16 00:00" HEAD)`. If no commit is that old, `typst-redline` stops with *--since needs a value*.

**From latexdiff-vc.** `-r v1` is `--since v1`, `-r v1 -r v2` is `--since v1 --to v2`, and `-L old -L new` names the two versions on the summary page. Revisions are always read with git, included files are always compared and the output is always a PDF, so `--git`, `--flatten` and `--pdf` are not needed.

**What a branch changed**: `typst-redline paper.typ --since main...feature` compares the point where `feature` left `main` with `feature`.

**Who changed what.** `--each-commit` gives each commit between the two versions its own colour, for up to 6 commits that change the document or a file it reads. The commits are listed in their colours on the summary page and in one small line in the bottom margin of each marked page ([picture](gallery.md#who-changed-what)).

```
typst-redline paper.typ --since sent-to-anna --each-commit
```

For more commits, `--steps` groups them into up to 6 steps, each in its own colour: `--steps 3` splits them into three steps that change about the same amount, and `--steps a1b2c3,submitted` ends steps at those revisions, the last step at the new version, uncommitted changes included. The summary lists each step with its range, date and commit subjects.

Only the first and the last version are compared, so text that one commit added and a later one removed does not appear, and a passage that several commits changed takes the colour of the commit that changed most of it. Both options need the old revision to be an ancestor of the new one, and do not work with two plain files.
