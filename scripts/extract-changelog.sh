#!/usr/bin/env bash
set -euo pipefail

# scripts/extract-changelog.sh <version>
#
# Prints the CHANGELOG.md section for <version> (no leading "v"), e.g.:
#   scripts/extract-changelog.sh 0.1.0
# Used to build release notes (release.yml's create-release job, Chunk 3)
# and to preview a section locally before tagging. Exits non-zero with
# nothing on stdout if the section doesn't exist or is empty, so callers
# fail loudly instead of publishing an empty release body.

source "$(dirname "${BASH_SOURCE[0]}")/lib/changelog-section.sh"

if [ "$#" -ne 1 ]; then
  echo "usage: extract-changelog.sh <version>" >&2
  exit 1
fi

version="$1"
cd "$(git rev-parse --show-toplevel)"

if [ ! -f CHANGELOG.md ]; then
  echo "CHANGELOG.md not found" >&2
  exit 1
fi

section="$(changelog_section "$version")"

if [ -z "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
  echo "CHANGELOG.md has no non-empty '## [$version] - YYYY-MM-DD' section" >&2
  exit 1
fi

printf '%s\n' "$section"
