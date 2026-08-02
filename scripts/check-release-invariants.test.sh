#!/usr/bin/env bash
set -euo pipefail

# scripts/check-release-invariants.test.sh
#
# A guardrail script has broken silently in this repo before (see the
# hardcoded-accent-hex guard's own exclusion-pattern bug) -- this exercises
# check-release-invariants.sh against controlled fixture states so a future
# edit to it can't regress silently the same way.

pass_count=0
fail_count=0

make_fixture() {
  fixture="$(mktemp -d)"
  git init -q "$fixture"
  mkdir -p "$fixture/apps/desktop/src-tauri"
  cat > "$fixture/apps/desktop/src-tauri/tauri.conf.json" <<'EOF'
{
  "productName": "magpie"
}
EOF
  cat > "$fixture/apps/desktop/package.json" <<'EOF'
{
  "name": "desktop",
  "private": true
}
EOF
  cat > "$fixture/Cargo.toml" <<'EOF'
[workspace.package]
version = "0.1.0"
edition = "2021"
EOF
  cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-02

- Initial release.
EOF
}

assert_exit() {
  local description="$1"
  local expected="$2"
  shift 2
  local actual=0
  ( cd "$fixture" && "$script" "$@" ) >/tmp/check-release-invariants.test.out 2>&1 || actual="$?"
  if [ "$actual" -eq "$expected" ]; then
    echo "ok - $description"
    pass_count=$((pass_count + 1))
  else
    echo "FAIL - $description (expected exit $expected, got $actual)"
    cat /tmp/check-release-invariants.test.out
    fail_count=$((fail_count + 1))
  fi
  rm -rf "$fixture"
}

repo_root="$(git rev-parse --show-toplevel)"
script="$repo_root/scripts/check-release-invariants.sh"

make_fixture
assert_exit "good state, no-arg mode passes" 0

make_fixture
echo '{"version": "0.1.0", "productName": "magpie"}' > "$fixture/apps/desktop/src-tauri/tauri.conf.json"
assert_exit "stray version in tauri.conf.json fails" 1

make_fixture
echo '{"name": "desktop", "private": true, "version": "0.1.0"}' > "$fixture/apps/desktop/package.json"
assert_exit "stray version in package.json fails" 1

make_fixture
rm "$fixture/CHANGELOG.md"
assert_exit "missing CHANGELOG.md fails" 1

make_fixture
cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [0.1.0] - 2026-08-02

- Initial release.
EOF
assert_exit "CHANGELOG.md missing Unreleased heading fails" 1

make_fixture
assert_exit "tag matches Cargo.toml version, changelog section filled: passes" 0 v0.1.0

make_fixture
assert_exit "tag does not match Cargo.toml version fails" 1 v0.2.0

make_fixture
cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-02

EOF
assert_exit "tag's changelog section exists but is empty fails" 1 v0.1.0

make_fixture
assert_exit "prerelease-suffixed tag fails via the ordinary version-match check (no special-casing needed)" 1 v0.1.0-rc.1

echo
echo "$pass_count passed, $fail_count failed"
[ "$fail_count" -eq 0 ]
