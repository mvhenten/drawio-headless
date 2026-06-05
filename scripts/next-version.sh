#!/usr/bin/env bash
# Compute the next semver from Conventional Commits since the last v* tag.
#
#   fix:            -> patch
#   feat:           -> minor
#   BREAKING CHANGE -> major  (either a `!` after type/scope, or a
#                              `BREAKING CHANGE:` footer)
#   anything else   -> no bump
#
# Prints the next version WITHOUT a leading `v` (e.g. `0.2.0`) on stdout and
# exits 0 when there is something to release. Prints nothing and exits 10 when
# no releasable commit is found since the last tag (the caller treats this as a
# no-op). Any other non-zero exit is a real error.
#
# Usage:
#   scripts/next-version.sh            # range = <last tag>..HEAD
#   scripts/next-version.sh <ref>      # range = <last tag>..<ref>
set -euo pipefail

cd "$(dirname "$0")/.."

head_ref="${1:-HEAD}"

# Most recent v* tag reachable from head_ref. If there is no tag yet, fall back
# to the repo root so the whole history is considered.
if last_tag="$(git describe --tags --match 'v*' --abbrev=0 "$head_ref" 2>/dev/null)"; then
    range="${last_tag}..${head_ref}"
    base_version="${last_tag#v}"
else
    last_tag=""
    range="$head_ref"
    base_version="0.0.0"
fi

# Walk each commit message in the range. `%B` is the full message (subject +
# body) so we can also catch a `BREAKING CHANGE:` footer; a NUL terminates each
# message so a multi-line body never bleeds into the next commit. We feed
# `git log` straight into the loop via process substitution — capturing it in a
# variable first would strip the NULs.
bump="none"
while IFS= read -r -d '' msg; do
    [ -z "$msg" ] && continue
    subject="${msg%%$'\n'*}"

    # Breaking change: `type!:` / `type(scope)!:` in the subject, or a
    # `BREAKING CHANGE:` / `BREAKING-CHANGE:` footer anywhere in the body.
    if printf '%s' "$subject" | grep -Eq '^[a-zA-Z]+(\([^)]*\))?!:' \
        || printf '%s' "$msg" | grep -Eq '^BREAKING[ -]CHANGE:'; then
        bump="major"
        break          # major dominates; nothing higher to find.
    fi

    case "$subject" in
        feat:*|feat\(*\):*|feat\(*\)!:*)
            [ "$bump" != "minor" ] && bump="minor" ;;
        fix:*|fix\(*\):*)
            [ "$bump" = "none" ] && bump="patch" ;;
    esac
done < <(git log --format='%B%x00' "$range" 2>/dev/null)

if [ "$bump" = "none" ]; then
    exit 10
fi

IFS=. read -r major minor patch <<EOF
$base_version
EOF
# Strip any pre-release/build metadata suffix from patch just in case.
patch="${patch%%[-+]*}"

case "$bump" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
esac

printf '%s.%s.%s\n' "$major" "$minor" "$patch"
