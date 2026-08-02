#!/usr/bin/env bash
set -euo pipefail

# scripts/check-release-invariants.sh [tag]
#
# No-arg mode (run from ci.yml on every push): asserts the version
# single-source-of-truth invariant holds and that CHANGELOG.md has an
# Unreleased section to write into.
#
# Tag-arg mode (run locally by the maintainer before pushing a release
# tag, e.g. `scripts/check-release-invariants.sh v0.1.0`, and later by
# release.yml's guard job): additionally asserts the tag matches
# Cargo.toml's version and that CHANGELOG.md has a matching dated,
# non-empty section for it. A prerelease-suffixed tag (e.g. v0.2.0-rc.1)
# fails here too, via the ordinary version-match check -- this project has
# no prerelease-tag concept, and that's deliberately not special-cased.

source "$(dirname "${BASH_SOURCE[0]}")/lib/changelog-section.sh"

cd "$(git rev-parse --show-toplevel)"

failures=()

if grep -q '"version"' apps/desktop/src-tauri/tauri.conf.json; then
  failures+=("apps/desktop/src-tauri/tauri.conf.json has a \"version\" key -- Cargo.toml's [workspace.package] version is the only source of truth, delete it")
fi

if grep -q '"version"' apps/desktop/package.json; then
  failures+=("apps/desktop/package.json has a \"version\" key -- Cargo.toml's [workspace.package] version is the only source of truth, delete it")
fi

if [ ! -f CHANGELOG.md ]; then
  failures+=("CHANGELOG.md is missing")
elif ! grep -q '^## \[Unreleased\]$' CHANGELOG.md; then
  failures+=("CHANGELOG.md has no '## [Unreleased]' heading")
fi

workspace_package_block="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f' Cargo.toml)"
cargo_version="$(printf '%s\n' "$workspace_package_block" | grep '^version = ' | head -1 | sed -E 's/^version = "(.*)"$/\1/' || true)"

if [ -z "$cargo_version" ]; then
  failures+=("Cargo.toml's [workspace.package] has no version key")
fi

if [ "$#" -ge 1 ]; then
  tag="$1"
  expected_version="${tag#v}"

  if [ "$expected_version" != "$cargo_version" ]; then
    failures+=("tag $tag (expects version $expected_version) does not match Cargo.toml's [workspace.package] version ($cargo_version)")
  fi

  if [ -f CHANGELOG.md ]; then
    section="$(changelog_section "$expected_version")"
    if [ -z "$(printf '%s' "$section" | tr -d '[:space:]')" ]; then
      failures+=("CHANGELOG.md has no non-empty '## [$expected_version] - YYYY-MM-DD' section for tag $tag")
    fi
  fi
fi

if [ "${#failures[@]}" -gt 0 ]; then
  echo "check-release-invariants.sh: FAILED" >&2
  for f in "${failures[@]}"; do
    echo "  - $f" >&2
  done
  exit 1
fi

if [ "$#" -ge 1 ]; then
  echo "check-release-invariants.sh: all invariants hold for tag $1."
else
  echo "check-release-invariants.sh: all invariants hold."
fi
