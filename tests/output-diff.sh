#!/bin/bash
# Runs two typst-redline binaries on every fixture in tests/fixtures and every example in docs/, in
# both layouts, and reports which output pages, kept sources and results (exit code, messages)
# differ. Writes <outdir>/summary.md, and for each difference the pictures before, after and their
# difference and the source diff under <outdir>/changes. Needs typst, git and pdftoppm on PATH;
# difference pictures also need ImageMagick's compare.
# Cases whose directory is missing from the old checkout <old-repo> (optional) are marked as new.
# The preamble the binaries embed is compared once; the kept sources are compared without it.
# Usage: tests/output-diff.sh <old-binary> <new-binary> <outdir> [<old-repo>]
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
bin_old=$(realpath "$1")
bin_new=$(realpath "$2")
mkdir -p "$3"
old_repo=${4:-${OLD_REPO:-}}
out=$(cd "$3" && pwd)
rm -rf "${out:?}"/{old,new,changes,cases,report,summary.md}
export TYPST_IGNORE_SYSTEM_FONTS=${TYPST_IGNORE_SYSTEM_FONTS:-true}

# The cases, one per line: <name> <directory> <arguments>.
for f in "$repo"/tests/fixtures/*/; do
  n=$(basename "$f")
  if [ -d "$f/old" ]; then echo "$n $f old/main.typ new/main.typ"; else echo "$n $f old.typ new.typ"; fi
done > "$out/cases"
for f in "$repo"/docs/*/old.typ; do
  echo "docs-$(basename "$(dirname "$f")") $(dirname "$f") old.typ new.typ" >> "$out/cases"
done
"$repo/docs/report/make-repo.sh" "$out/report"
echo "docs-report $out/report main.typ --since draft-1" >> "$out/cases"
echo "docs-report-each-commit $out/report main.typ --since draft-1 --each-commit" >> "$out/cases"

# sweep <old|new> <binary>: runs the binary on every case into $out/<old|new>/<case>/<layout>/
sweep() {
  while read -r name dir args; do
    for layout in inline side-by-side; do
      d="$out/$1/$name/$layout"
      mkdir -p "$d"
      # shellcheck disable=SC2086
      code=0; (cd "$dir" && "$2" $args -o "$d/out.pdf" --layout "$layout" --keep-typ) > /dev/null 2> "$d/stderr" || code=$?
      kept=$(sed -n 's/^kept //p' "$d/stderr" | tr -d '\r')
      if [ -n "$kept" ]; then
        if command -v cygpath > /dev/null; then kept=$(cygpath -u "$kept"); fi
        mv "$kept" "$d/typ"
        find "$d/typ" -name .redline-preamble.typ | sort | xargs -r cat > "$d/preamble.typ"
        find "$d/typ" -name .redline-preamble.typ -delete
        find "$d/typ" \( -name .redline-deps -o -name '.redline-*.pdf' \) -delete
      fi
      { echo "exit $code"; grep -v '^kept ' "$d/stderr" | sed -E 's/typst-redline-[0-9]+/typst-redline-N/g' || true; } > "$d/result"
      if [ -f "$d/out.pdf" ]; then pdftoppm -r 80 -png "$d/out.pdf" "$d/page"; fi
      for p in "$d"/page-*.png; do
        [ -e "$p" ] || continue
        n=${p##*-}; mv "$p" "$d/p$((10#${n%.png}))"
      done
    done
  done < "$out/cases"
}
sweep old "$bin_old" &
sweep new "$bin_new"
wait $!

pages=0 same=0 preamble=""
rows=""
while read -r name dir _; do
  for layout in inline side-by-side; do
    a="$out/old/$name/$layout" b="$out/new/$name/$layout" c="$out/changes/$name/$layout"
    changed="" new="" removed="" notes=""
    rel=${dir#"$repo"/}
    if [ -n "$old_repo" ] && [ "$rel" != "$dir" ] && [ "${dir#"$out"/}" = "$dir" ] && [ ! -d "$old_repo/$rel" ]; then notes="new case"; fi
    if [ -e "$a/preamble.typ" ] && ! cmp -s "$a/preamble.typ" "$b/preamble.typ"; then preamble=" The preamble differs between the two builds; the sources are compared without it."; fi
    for p in $( (ls "$a" "$b" 2> /dev/null | grep '^p[0-9]*$' | tr -d p | sort -nu) || true); do
      pages=$((pages + 1))
      if [ ! -e "$a/p$p" ]; then new="$new $p"
      elif [ ! -e "$b/p$p" ]; then removed="$removed $p"; mkdir -p "$c"; cp "$a/p$p" "$c/$p-before.png"; continue
      elif cmp -s "$a/p$p" "$b/p$p"; then same=$((same + 1)); continue
      else changed="$changed $p"; mkdir -p "$c"; cp "$a/p$p" "$c/$p-before.png"
        command -v compare > /dev/null && { compare "$a/p$p" "$b/p$p" "$c/$p-difference.png" 2> /dev/null || true; }
      fi
      mkdir -p "$c"; cp "$b/p$p" "$c/$p-after.png"
    done
    if { [ -d "$a/typ" ] || [ -d "$b/typ" ]; } && ! diff -r "$a/typ" "$b/typ" > "$out/typ.diff" 2>&1; then
      mkdir -p "$c"; mv "$out/typ.diff" "$c/source.diff"; notes="$notes source"
    fi
    if ! cmp -s "$a/result" "$b/result"; then
      mkdir -p "$c"; diff "$a/result" "$b/result" > "$c/result.diff" || true; notes="$notes result"
    fi
    if [ -n "$changed$new$removed$notes" ]; then
      rows="$rows| $name | $layout |${changed:- } |${new:- } |${removed:- } | $notes |"$'\n'
    fi
  done
done < "$out/cases"
rm -f "$out/typ.diff"

cases=$(wc -l < "$out/cases")
{
  echo "## Output differences"
  echo
  if [ -z "$rows" ] && [ -z "$preamble" ]; then
    echo "No output changed: $cases cases in two layouts, $pages pages."
  else
    echo "$(printf '%s' "$rows" | grep -c . || true) rows differ; $same of $pages pages unchanged, over $cases cases in two layouts.$preamble"
    echo
    echo "Pictures and source diffs of the differences are in the artifact."
    echo
    echo "| case | layout | changed pages | new pages | removed pages | also differs |"
    echo "|---|---|---|---|---|---|"
    printf '%s' "$rows"
  fi
} > "$out/summary.md"
cat "$out/summary.md"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then cat "$out/summary.md" >> "$GITHUB_STEP_SUMMARY"; fi
