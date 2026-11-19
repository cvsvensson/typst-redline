#!/bin/sh
# Makes every picture in docs/ from the sources next to it: runs typst-redline and renders pages
# of its PDF unchanged with pdftoppm (from poppler). Needs typst, git and pdftoppm on PATH.
# Usage: docs/make-pictures.sh [typst-redline binary, default target/release/typst-redline]
set -eu
docs=$(cd "$(dirname "$0")" && pwd)
redline=$(cd "$docs/.." && realpath "${1:-target/release/typst-redline}")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# run <dir> <args...>: runs typst-redline in <dir>, writing $tmp/out.pdf
run() {
  (cd "$1" && shift && "$redline" "$@" -o "$tmp/out.pdf" >/dev/null)
}

# picture <page, or "last"> <name> [ppi]: renders a page of $tmp/out.pdf to docs/<name>.png
picture() {
  page=$1
  [ "$page" = last ] && page=$(pdfinfo "$tmp/out.pdf" | sed -n 's/^Pages: *//p')
  pdftoppm -r "${3:-200}" -png -singlefile -f "$page" -l "$page" "$tmp/out.pdf" "$docs/$2"
}

run "$docs/paper" old.typ new.typ
picture 1 paper

run "$docs/moved" old.typ new.typ
picture 1 moved

run "$docs/slides" old.typ new.typ --layout side-by-side
picture 4 slides 150

# A git repository with one commit per folder of docs/report, each folder holding the files that
# commit changes. Fixed dates and author keep the commit hashes in the pictures the same.
repo="$tmp/report"
git init -q "$repo"
git -C "$repo" config core.autocrlf false
commit() {
  cp -r "$docs/report/$1/." "$repo/"
  git -C "$repo" add -A
  GIT_AUTHOR_DATE="$2T10:00:00Z" GIT_COMMITTER_DATE="$2T10:00:00Z" git -C "$repo" \
    -c user.name="Robin Lee" -c user.email=robin@example.com -c commit.gpgsign=false commit -q -m "$3"
}
commit 1 2026-11-02 "First draft"
git -C "$repo" tag draft-1
commit 2 2026-11-04 "Tighten the introduction"
commit 3 2026-11-06 "Rerun the benchmarks on the new board"
commit 4 2026-11-09 "Retitle, extend the abstract, cite Knuth"

run "$repo" main.typ --since draft-1
picture 3 report-results
picture last report-summary
run "$repo" main.typ --since draft-1 --each-commit
picture 3 report-commits
