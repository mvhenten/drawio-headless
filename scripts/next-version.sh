#!/usr/bin/env bash
# Compute the next semver from Conventional Commits since the last release tag.
#
#   fix:                              -> patch
#   feat:                             -> minor
#   `!` after type/scope, or a footer
#   `BREAKING CHANGE:` / `BREAKING-CHANGE:` -> major
#   anything else (docs, chore, ci, style, test, refactor, build, perf, ...)
#                                     -> no bump
#
# The highest bump across all commits in the range wins (major > minor >
# patch). Prints the next version WITHOUT a leading `v` (e.g. `0.2.0`) on
# stdout and exits 0 when there is something to release. Prints nothing and
# exits 10 when no releasable commit exists since the last tag (the caller
# treats this as a no-op / skip). Any other non-zero exit is a real error.
#
# Requires a full clone (fetch-depth 0, tags included) to see the last tag and
# the full commit range.
#
# Usage: scripts/next-version.sh
set -euo pipefail

cd "$(dirname "$0")/.."

last_tag="$(git tag --merged HEAD -l 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -n1 || true)"

if [ -z "$last_tag" ]; then
  base="0.0.0"
  range="HEAD"
else
  base="${last_tag#v}"
  range="${last_tag}..HEAD"
fi

bump="none"

while IFS= read -r -d $'\x02' record; do
  [ -z "$record" ] && continue
  subject="${record%%$'\x01'*}"
  body="${record#*$'\x01'}"

  type=""
  bang=""
  if [[ "$subject" =~ ^([a-zA-Z]+)(\([^\)]*\))?(!)?:[[:space:]] ]]; then
    type="${BASH_REMATCH[1]}"
    bang="${BASH_REMATCH[3]}"
  fi

  is_breaking=false
  if [ -n "$bang" ] || printf '%s\n%s' "$subject" "$body" | grep -qE '^BREAKING[ -]CHANGE:'; then
    is_breaking=true
  fi

  if $is_breaking; then
    bump="major"
  elif [ "$bump" != "major" ] && [ "$type" = "feat" ]; then
    bump="minor"
  elif [ "$bump" = "none" ] && [ "$type" = "fix" ]; then
    bump="patch"
  fi
done < <(git log --format='%s%x01%b%x02' "$range")

if [ "$bump" = "none" ]; then
  echo "no releasable commits since ${last_tag:-<repo start>}" >&2
  exit 10
fi

IFS='.' read -r major minor patch <<< "$base"

case "$bump" in
  major) major=$((major + 1)); minor=0; patch=0 ;;
  minor) minor=$((minor + 1)); patch=0 ;;
  patch) patch=$((patch + 1)) ;;
esac

echo "${major}.${minor}.${patch}"
