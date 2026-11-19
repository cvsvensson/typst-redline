#!/bin/sh
# Makes the git repository <dir> with one commit per numbered folder next to this script, each
# folder holding the files that commit changes, and the tag draft-1 on the first. Fixed dates and
# author keep the commit hashes the same.
# Usage: docs/report/make-repo.sh <dir>
set -eu
src=$(cd "$(dirname "$0")" && pwd)
repo=$1
git init -q "$repo"
git -C "$repo" config core.autocrlf false
commit() {
  cp -r "$src/$1/." "$repo/"
  git -C "$repo" add -A
  GIT_AUTHOR_DATE="$2T10:00:00Z" GIT_COMMITTER_DATE="$2T10:00:00Z" git -C "$repo" \
    -c user.name="Robin Lee" -c user.email=robin@example.com -c commit.gpgsign=false commit -q -m "$3"
}
commit 1 2026-11-02 "First draft"
git -C "$repo" tag draft-1
commit 2 2026-11-04 "Tighten the introduction"
commit 3 2026-11-06 "Rerun the benchmarks on the new board"
commit 4 2026-11-09 "Retitle, extend the abstract, cite Knuth"
