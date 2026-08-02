# Release Pipeline — Chunk 1: Version Single Source of Truth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse magpie's three independently-tracked version strings down to one (`Cargo.toml`'s `[workspace.package] version`), and add the guardrail scripts and CI wiring that make that invariant self-enforcing. This is Chunk 1 of four independently-mergeable chunks building toward a full release + auto-update pipeline — see the full design doc at `/Users/akshaykrishna/.claude-work/plans/lets-do-it-lets-shiny-rossum.md` for context on Chunks 2–4 and the reasoning behind every decision below.

**Architecture:** Tauri v2 already falls back to `Cargo.toml`'s version when `tauri.conf.json` has no `version` key (verified directly against `tauri-utils` source during design), so deleting the two redundant `version` keys is enough to make `Cargo.toml` the sole source of truth — no code needs to read a version from anywhere else. Two new scripts (`check-release-invariants.sh`, `extract-changelog.sh`) share a small `lib/changelog-section.sh` helper so the "what counts as a version's changelog section" logic exists in exactly one place. `check-release-invariants.sh` runs in two modes: no-arg (wired into `ci.yml`, checked on every push) and tag-arg (run locally by the maintainer before pushing a release tag, and later reused by Chunk 3's `release.yml` `guard` job).

**Tech Stack:** Bash (`scripts/`), GitHub Actions (`.github/workflows/ci.yml`), Cargo workspace versioning, Tauri v2 config.

## Global Constraints

- **`Cargo.toml`'s `[workspace.package] version` is the only place a version number exists in the repo.** No fallback logic anywhere else — this is documented, verified Tauri v2 behavior, not an assumption.
- **`CHANGELOG.md` is created in this chunk, not later.** A real sequencing bug was caught during design review: the guardrail script this chunk wires into CI requires the file to exist, and CI must stay green if this chunk merges alone (the whole point of "independently-mergeable chunks"). Only a minimal stub goes in now (Keep a Changelog header + an empty `## [Unreleased]` heading); a later chunk adds the explanatory prose and real dated release content.
- **Scripts must work on both `bash` on Linux (GNU coreutils/awk/sed) and `bash` on macOS (BSD awk/sed).** `ci.yml`'s `lint-and-test` job runs on both `ubuntu-latest` and `macos-latest`, and the new steps this chunk adds run unguarded on both, so any GNU/BSD dialect divergence surfaces immediately in CI.
- **No new test framework or dependency.** This repo has no existing shell-script test tooling; `check-release-invariants.test.sh` is a self-contained bash script using plain fixture directories and exit-code assertions, not bats or any other framework.
- Match existing code style: comments explain *why*, not *what* (see `ci.yml`'s existing accent-hex guardrail step for the house style already used for CI guardrails in this repo).

---

### Task 1: Collapse the three version strings into one source of truth

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `apps/desktop/package.json`

**Interfaces:**
- Produces: `Cargo.toml`'s `[workspace.package] version` (currently `"0.1.0"`, unchanged by this task) becomes the sole version string in the repo, consumed by Task 3/4's scripts and, later, Chunk 3's `release.yml`.

- [ ] **Step 1: Delete the redundant `version` key from `tauri.conf.json`**

In `apps/desktop/src-tauri/tauri.conf.json`, change:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "magpie",
  "version": "0.1.0",
  "identifier": "app.magpie.desktop",
```

to:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "magpie",
  "identifier": "app.magpie.desktop",
```

- [ ] **Step 2: Delete the redundant `version` key from `package.json`**

In `apps/desktop/package.json`, change:

```json
{
  "name": "desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
```

to:

```json
{
  "name": "desktop",
  "private": true,
  "type": "module",
```

- [ ] **Step 3: Confirm nothing else in the repo reads either deleted key**

Run: `grep -rn '"version"' apps/desktop/src apps/desktop/vite.config.ts apps/desktop/src-tauri/src 2>/dev/null`

Expected: no output. (Already verified during design — `apps/desktop/package.json` is `"private": true` and nothing in `apps/desktop/src` or `vite.config.ts` reads its `version` field; this step re-confirms it hasn't changed since.)

- [ ] **Step 4: Fast build check**

Run: `cargo build --workspace`

Expected: succeeds. This doesn't yet prove the *bundle* is versioned correctly (that needs a full `pnpm tauri build`, done once at the end of this chunk in Task 5) — it just confirms the two JSON edits didn't break anything Cargo cares about.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/tauri.conf.json apps/desktop/package.json
git commit -m "Remove redundant version keys; Cargo.toml is now the sole version source"
```

---

### Task 2: Minimal `CHANGELOG.md` stub

**Files:**
- Create: `CHANGELOG.md`

**Interfaces:**
- Produces: a file with a literal `## [Unreleased]` heading line, consumed by Task 3's `check-release-invariants.sh` (no-arg mode) and, later, by whichever chunk adds real dated sections.

- [ ] **Step 1: Create the stub**

Create `CHANGELOG.md`:

```markdown
# Changelog

## [Unreleased]
```

- [ ] **Step 2: Verify the heading is byte-exact**

Run: `grep -n '^## \[Unreleased\]$' CHANGELOG.md`

Expected: prints `3:## [Unreleased]` (must match exactly — no trailing text on the line — since Task 3's script checks for this exact line).

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "Add minimal CHANGELOG.md stub (Keep a Changelog format)"
```

---

### Task 3: Shared changelog-section helper + `check-release-invariants.sh` + fixture tests

**Files:**
- Create: `scripts/lib/changelog-section.sh`
- Create: `scripts/check-release-invariants.sh`
- Create: `scripts/check-release-invariants.test.sh`

**Interfaces:**
- Produces: shell function `changelog_section <version>` (in `scripts/lib/changelog-section.sh`) — prints `CHANGELOG.md`'s `## [<version>] - YYYY-MM-DD` section (empty output if absent/empty), reading `CHANGELOG.md` from the current working directory. Consumed by Task 4's `extract-changelog.sh` and, in Chunk 3, by `release.yml`.
- Produces: `scripts/check-release-invariants.sh [tag]` — exit 0 if invariants hold, exit 1 with failures listed on stderr otherwise. Consumed by Task 5's `ci.yml` wiring, by the maintainer locally before tagging, and by Chunk 3's `release.yml` `guard` job.

- [ ] **Step 1: Write the fixture test script first (it will fail — the script under test doesn't exist yet)**

Create `scripts/check-release-invariants.test.sh`:

```bash
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
```

- [ ] **Step 2: Make it executable and run it — confirm it fails because `check-release-invariants.sh` doesn't exist yet**

Run:
```bash
chmod +x scripts/check-release-invariants.test.sh
scripts/check-release-invariants.test.sh
```

Expected: every case prints `FAIL` (the underlying `"$script"` call errors with "No such file or directory"), and the script exits non-zero on the final `[ "$fail_count" -eq 0 ]`.

- [ ] **Step 3: Implement the shared changelog-section helper**

Create `scripts/lib/changelog-section.sh`:

```bash
# scripts/lib/changelog-section.sh
#
# changelog_section <version>: prints CHANGELOG.md's "## [<version>] -
# YYYY-MM-DD" section (empty output if the section doesn't exist or has no
# entries under it). Reads CHANGELOG.md from the current working directory.
# Shared by check-release-invariants.sh and extract-changelog.sh so the two
# never drift on what counts as "the changelog section for a version" --
# exactly the kind of guardrail-logic duplication that's broken silently in
# this repo before (see the accent-hex guard's own exclusion-pattern bug).
changelog_section() {
  local version="$1"
  awk -v ver="$version" '
    $0 ~ ("^## \\[" ver "\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$") { in_section=1; next }
    /^## / { in_section=0 }
    in_section { print }
  ' CHANGELOG.md
}
```

- [ ] **Step 4: Implement `check-release-invariants.sh`**

Create `scripts/check-release-invariants.sh`:

```bash
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
```

- [ ] **Step 5: Make it executable and run the fixture tests again — confirm all pass**

Run:
```bash
chmod +x scripts/check-release-invariants.sh
scripts/check-release-invariants.test.sh
```

Expected: nine `ok -` lines, `9 passed, 0 failed`, exit 0.

- [ ] **Step 6: Run it for real against this repo, in both modes**

Run:
```bash
scripts/check-release-invariants.sh
scripts/check-release-invariants.sh v0.1.0
```

Expected: the first (no-arg) command prints `all invariants hold.` and exits 0. The second is **expected to fail** — this repo's real `CHANGELOG.md` (Task 2) has an `## [Unreleased]` heading but no `## [0.1.0] - ...` dated section yet (a later chunk writes that). Confirm it fails with exactly: `CHANGELOG.md has no non-empty '## [0.1.0] - YYYY-MM-DD' section for tag v0.1.0` — not a different, unexpected error.

- [ ] **Step 7: Commit**

```bash
git add scripts/lib/changelog-section.sh scripts/check-release-invariants.sh scripts/check-release-invariants.test.sh
git commit -m "Add check-release-invariants.sh with fixture tests"
```

---

### Task 4: `extract-changelog.sh`

**Files:**
- Create: `scripts/extract-changelog.sh`

**Interfaces:**
- Consumes: `changelog_section <version>` from `scripts/lib/changelog-section.sh` (Task 3).
- Produces: `scripts/extract-changelog.sh <version>` — prints that version's changelog section to stdout, exit 1 if missing/empty. Consumed later by Chunk 3's `release.yml` `create-release` job.

- [ ] **Step 1: Implement it**

Create `scripts/extract-changelog.sh`:

```bash
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
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/extract-changelog.sh`

- [ ] **Step 3: Verify against a real, filled section**

Run: `scripts/extract-changelog.sh 0.1.0`

Expected: exits 1, prints `CHANGELOG.md has no non-empty '## [0.1.0] - YYYY-MM-DD' section` to stderr — correct, matching Task 3 Step 6 (no real `0.1.0` section exists yet). To confirm the happy path works, run it against a temporary fixture instead:

```bash
fixture="$(mktemp -d)"
cat > "$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-02

- Initial release.
EOF
( cd "$fixture" && git init -q && "$OLDPWD/scripts/extract-changelog.sh" 0.1.0 )
rm -rf "$fixture"
```

Expected: prints:
```

- Initial release.

```
(the section content between the `## [0.1.0] - 2026-08-02` heading and EOF) and exits 0.

- [ ] **Step 4: Commit**

```bash
git add scripts/extract-changelog.sh
git commit -m "Add extract-changelog.sh"
```

---

### Task 5: Wire into CI, then full chunk verification

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `scripts/check-release-invariants.sh`, `scripts/check-release-invariants.test.sh` (Task 3).

- [ ] **Step 1: Add the two new steps to `lint-and-test`**

In `.github/workflows/ci.yml`, change:

```yaml
      - name: Guard against hardcoded accent hex outside tokens.css
        working-directory: apps/desktop/src
        run: |
          if grep -rn '3E7A92\|3e7a92\|123A4B\|123a4b' . --include='*.ts' --include='*.tsx' --include='*.css' --include='*.html' | grep -v 'styles/tokens\.css:'; then
            echo "::error::Hardcoded accent hex found outside styles/tokens.css -- use var(--mp-accent) (or tokens.css's --mp-accent-on-light/-on-dark for the rare fixed-color case) instead."
            exit 1
          fi

  build:
```

to:

```yaml
      - name: Guard against hardcoded accent hex outside tokens.css
        working-directory: apps/desktop/src
        run: |
          if grep -rn '3E7A92\|3e7a92\|123A4B\|123a4b' . --include='*.ts' --include='*.tsx' --include='*.css' --include='*.html' | grep -v 'styles/tokens\.css:'; then
            echo "::error::Hardcoded accent hex found outside styles/tokens.css -- use var(--mp-accent) (or tokens.css's --mp-accent-on-light/-on-dark for the rare fixed-color case) instead."
            exit 1
          fi

      # A guardrail script has broken silently in this repo before (see
      # the accent-hex guard's own exclusion-pattern bug) -- test the
      # tester before trusting its output on real code.
      - name: Test check-release-invariants.sh
        run: scripts/check-release-invariants.test.sh

      # Enforces the version-single-source-of-truth and changelog
      # discipline the release pipeline depends on, checked on every push
      # so it can never silently drift.
      - name: Check release invariants
        run: scripts/check-release-invariants.sh

  build:
```

- [ ] **Step 2: Commit the CI change**

```bash
git add .github/workflows/ci.yml
git commit -m "Wire check-release-invariants.sh into ci.yml's lint-and-test job"
```

- [ ] **Step 3: Full chunk verification — `cargo build --workspace`**

Run: `cargo build --workspace`

Expected: succeeds.

- [ ] **Step 4: Full chunk verification — real `pnpm tauri build`, correctly versioned**

Run:
```bash
cd apps/desktop
pnpm install --frozen-lockfile
pnpm tauri build
```

Expected: succeeds and produces a bundle whose filename embeds `0.1.0` (e.g. `target/release/bundle/dmg/magpie_0.1.0_aarch64.dmg` on macOS, or the `.deb`/`.AppImage` equivalents on Linux) — proving Tauri's fallback to `Cargo.toml`'s version (Task 1's key deletions) actually works, not just compiles.

On macOS, additionally run:
```bash
/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "target/release/bundle/macos/magpie.app/Contents/Info.plist"
```
Expected: prints `0.1.0`.

- [ ] **Step 5: Confirm CI is green**

Push the branch (or open a PR) and confirm the `lint-and-test` job's two new steps both pass on both `ubuntu-latest` and `macos-latest`.

---

## Chunk Verification Summary

- `scripts/check-release-invariants.test.sh` passes locally (9/9) — Task 3.
- `scripts/check-release-invariants.sh` (no-arg) passes locally and in CI — Task 5.
- `scripts/check-release-invariants.sh v0.1.0` correctly fails today (no real `0.1.0` changelog section yet) with the expected, specific error — Task 3/4.
- `cargo build --workspace` and a real `pnpm tauri build` both succeed, with the built bundle and `Info.plist` (macOS) reflecting `0.1.0` — Task 5.
- No behavior in this chunk depends on any secret, external repo, or the real signing key — matches the design doc's "no secrets, no external anything" scope for Chunk 1.
