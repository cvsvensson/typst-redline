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

for t in UNDERLINE COLOR CULINECHBAR CCHANGEBAR CHANGEBAR; do
  run "$docs/paper" old.typ new.typ -t $t
  picture 1 "paper-$(echo $t | tr A-Z a-z)" 120
done
run "$docs/paper" old.typ new.typ --style ins=purple --style ins-fill=none
picture 1 paper-style 120

run "$docs/moved" old.typ new.typ
picture 1 moved

run "$docs/footnotes" old.typ new.typ
picture 1 footnotes

run "$docs/slides" old.typ new.typ --layout side-by-side
picture 4 slides 150

repo="$tmp/report"
"$docs/report/make-repo.sh" "$repo"

run "$repo" main.typ --since draft-1
picture 3 report-results
picture last report-summary
run "$repo" main.typ --since draft-1 --each-commit
picture 3 report-commits
