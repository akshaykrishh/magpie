#!/usr/bin/env bash
set -euo pipefail

# scripts/extract-changelog.test.sh
#
# extract-changelog.sh will directly generate GitHub release body text in a
# later chunk, so it gets the same fixture-test treatment as
# check-release-invariants.sh instead of relying on ad-hoc manual
# verification -- a guardrail script has broken silently in this repo
# before (see the accent-hex guard's own exclusion-pattern bug).

pass_count=0
fail_count=0

make_fixture() {
  fixture="$(mktemp -d)"
  git init -q "$fixture"
  cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-02

- Initial release.
EOF
}

assert() {
  local description="$1"
  local expected_exit="$2"
  local expected_output="$3"
  shift 3
  local actual_exit=0
  local actual_output
  actual_output="$( cd "$fixture" && "$script" "$@" 2>&1 )" || actual_exit="$?"
  if [ "$actual_exit" -eq "$expected_exit" ] && [ "$actual_output" = "$expected_output" ]; then
    echo "ok - $description"
    pass_count=$((pass_count + 1))
  else
    echo "FAIL - $description (expected exit $expected_exit, got $actual_exit)"
    echo "  --- expected output ---"
    printf '%s\n' "$expected_output"
    echo "  --- actual output ---"
    printf '%s\n' "$actual_output"
    fail_count=$((fail_count + 1))
  fi
  rm -rf "$fixture"
}

repo_root="$(git rev-parse --show-toplevel)"
script="$repo_root/scripts/extract-changelog.sh"

make_fixture
assert "version section exists and is non-empty: prints section, exits 0" \
  0 "$(printf '\n- Initial release.')" 0.1.0

make_fixture
assert "version section does not exist: fails with documented message" \
  1 "CHANGELOG.md has no non-empty '## [9.9.9] - YYYY-MM-DD' section" 9.9.9

make_fixture
cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-02

EOF
assert "version section exists but is empty: fails with documented message" \
  1 "CHANGELOG.md has no non-empty '## [0.1.0] - YYYY-MM-DD' section" 0.1.0

make_fixture
rm "$fixture/CHANGELOG.md"
assert "CHANGELOG.md missing entirely: fails" 1 "CHANGELOG.md not found" 0.1.0

make_fixture
assert "zero arguments: fails with usage message" \
  1 "usage: extract-changelog.sh <version>"

make_fixture
assert "two or more arguments: fails with usage message" \
  1 "usage: extract-changelog.sh <version>" 0.1.0 0.2.0

echo
echo "$pass_count passed, $fail_count failed"
[ "$fail_count" -eq 0 ]
