# Release Pipeline — Chunk 3: `release.yml` and Real Signing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the automated tag-push-to-published-release pipeline: a new `.github/workflows/release.yml` (guard → create-release → build → verify-bundle → checksums-and-attest → publish, fully automatic once a tag is pushed), the `tauri.release.conf.json` overlay that turns on updater-artifact signing, and the macOS/Linux bundle config it needs — then actually cut the real `v0.1.0` tag (Linux-only; macOS stays gated on `vars.MACOS_RELEASE_ENABLED`, unset until Phase 3) as this chunk's own end-to-end proof. This is Chunk 3 of four — see `/Users/akshaykrishna/.claude-work/plans/lets-do-it-lets-shiny-rossum.md` for the full four-chunk design and every decision's reasoning (referenced by name below, not re-derived). Unlike Chunks 1 and 2, **this chunk genuinely depends on both of them already being merged** — it reuses Chunk 1's `check-release-invariants.sh`/`extract-changelog.sh` (tag-arg mode, already built) and Chunk 2's `updater.rs`/`plugins.updater` placeholder pubkey (which this chunk replaces with the real one). Both are merged to `main` as of this plan.

**Architecture:** `release.yml` triggers on `push: tags: ["v*"]` and `workflow_dispatch`. Five jobs, each gated by `needs:` on the previous: `guard` (checkout at the tag, run `check-release-invariants.sh` with the tag, assert the tag's commit is on `main`, compute the build matrix as JSON) → `create-release` (self-healing draft creation, from Chunk 3's own grilling-pass decision) → `build` (matrix: Linux always, macOS gated on `vars.MACOS_RELEASE_ENABLED`; mirrors `ci.yml`'s build job, adds universal-binary Rust targets for macOS, runs `tauri-action` with the real signing key via env vars, then a `verify-bundle` step — artifact integrity, not a GUI launch, per that grilling-pass decision) → `checksums-and-attest` (downloads exactly what's attached to the draft, `SHA256SUMS`, `actions/attest-build-provenance`) → `publish` (flips the draft live, runs automatically once every prior job succeeds — no manual trigger, per the "full automation" decision). Every third-party action is pinned to a full commit SHA (resolved and verified against the real repos below, not guessed) and every job declares exactly the `permissions:` it needs.

**Tech Stack:** GitHub Actions (`.github/workflows/release.yml`), `tauri-apps/tauri-action`, Tauri v2 bundle config, Ed25519 updater signing.

## Global Constraints

- **A real Ed25519 signing keypair must exist before Task 5.** This is Phase 0 Step 1 of the master plan — the user runs `pnpm tauri signer generate -w ~/.magpie/updater.key` themselves (a private key an agent must never generate, see, or handle) and gives the implementer the **public** key plus sets the **private** key and its passphrase as GitHub repo secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). Tasks 1-4 do not need this — the placeholder pubkey from Chunk 2 keeps working for everything except Task 5's real end-to-end cut. Do not proceed to Task 5 until this is confirmed done.
- **Cutting the real `v0.1.0` tag (Task 5) publishes a real, public GitHub Release and is genuinely hard to reverse.** Get explicit human confirmation immediately before pushing the tag — this is not a step to run through automatically as "verification."
- **`MACOS_RELEASE_ENABLED` stays unset for this chunk.** Phase 3 (macOS + Homebrew activation) is out of scope — blocked on Apple Developer Program enrollment (Phase 0 steps 3-4), which the user hasn't done yet. `release.yml`'s `build` matrix must simply not contain a macOS entry when unset, not exist-and-skip.
- **Every third-party action pinned to a full commit SHA**, not a mutable tag — a mutable tag in a workflow holding the signing key is exactly the xz-shaped hole this pipeline exists to avoid. Every SHA below was resolved directly against the real GitHub repos during planning (`gh api repos/<owner>/<repo>/commits/<ref>`), not guessed:
  - `actions/checkout@v4` → `11d5960a326750d5838078e36cf38b85af677262`
  - `dtolnay/rust-toolchain@stable` → `4cda84d5c5c54efe2404f9d843567869ab1699d4`
  - `Swatinem/rust-cache@v2` → `e18b497796c12c097a38f9edb9d0641fb99eee32`
  - `pnpm/action-setup@v4` → `b906affcce14559ad1aafd4ab0e942779e9f58b1`
  - `actions/setup-node@v4` → `49933ea5288caeca8642d1e84afbd3f7d6820020`
  - `tauri-apps/tauri-action@v0` → `84b9d35b5fc46c1e45415bdb6144030364f7ebc5`
  - `actions/attest-build-provenance@v2` → `e8998f949152b193b063cb0ec769d69d929409be`
- **Least-privilege `permissions:` per job**, matching each job's actual needs — `guard`: `contents: read`; `create-release`/`build`/`publish`: `contents: write`; `checksums-and-attest`: `contents: write` + `id-token: write` + `attestations: write`.
- Match existing code style: comments explain *why*, not *what* — this repo's release-critical scripts (`check-release-invariants.sh`) already set the precedent for heavily-commented, safety-conscious shell/YAML.

---

### Task 1: Bundle config — entitlements, macOS/Linux targets, the updater-artifacts overlay

**Files:**
- Create: `apps/desktop/src-tauri/entitlements.plist`
- Create: `apps/desktop/src-tauri/tauri.release.conf.json`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: `tauri.release.conf.json` (the `createUpdaterArtifacts` overlay), consumed by Task 4's `build` job via `tauri-action`'s `--config` flag. `entitlements.plist`, referenced by `tauri.conf.json`'s new `bundle.macOS.entitlements` key.

- [ ] **Step 1: Create a minimal entitlements file**

Create `apps/desktop/src-tauri/entitlements.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
</dict>
</plist>
```

Deliberately empty — the real entitlement set can only be determined empirically in Phase 3 against a real Developer ID certificate; guessing and commenting it confidently now would just be wrong later. A near-empty file is enough for the config key to be valid today.

- [ ] **Step 2: Add the macOS/Linux bundle config**

In `apps/desktop/src-tauri/tauri.conf.json`, change:

```json
    "copyright": "Copyright the magpie contributors",
    "homepage": "https://github.com/akshaykrishh/magpie"
  },
  "plugins": {
```

to:

```json
    "copyright": "Copyright the magpie contributors",
    "homepage": "https://github.com/akshaykrishh/magpie",
    "macOS": {
      "entitlements": "entitlements.plist",
      "minimumSystemVersion": "11.0"
    },
    "linux": {
      "deb": {
        "depends": ["libayatana-appindicator3-1"]
      }
    }
  },
  "plugins": {
```

`minimumSystemVersion: "11.0"` (Big Sur) is a pragmatic floor for a 2026 dev-tool audience, matched later by the Homebrew cask's `depends_on macos:` in Phase 3. `libayatana-appindicator3-1` is explicit because Tauri doesn't reliably auto-detect it, and this app is tray-resident (`apps/desktop/src-tauri/src/tray.rs:39`) — a `.deb` that installs with no menu-bar icon would be the worst possible first impression on the one platform (Linux) nobody's hand-verified yet.

- [ ] **Step 3: Create the updater-artifacts overlay**

Create `apps/desktop/src-tauri/tauri.release.conf.json`:

```json
{
  "bundle": {
    "createUpdaterArtifacts": true
  }
}
```

**Load-bearing**: this stays a separate overlay file, applied only by `release.yml` via `--config`, not merged into the base `tauri.conf.json`. Putting it in the base config would break every contributor's local `pnpm tauri build` and `ci.yml`'s existing unsigned build job — both would demand a private signing key they don't have.

- [ ] **Step 4: Verify**

Run: `cargo build --workspace` (confirms the JSON is still valid and Tauri's config loader accepts it) and `pnpm --dir apps/desktop exec tsc --noEmit` (confirms nothing frontend-side broke).

Expected: both clean. Do **not** run `pnpm tauri build` with `--config tauri.release.conf.json` yet — that requires the real signing key, not available until Task 5.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri/entitlements.plist apps/desktop/src-tauri/tauri.release.conf.json apps/desktop/src-tauri/tauri.conf.json
git commit -m "Add release bundle config: entitlements, macOS/Linux targets, updater-artifacts overlay"
```

---

### Task 2: The real `0.1.0` CHANGELOG entry

**Files:**
- Modify: `CHANGELOG.md`

**Interfaces:**
- Produces: a `## [0.1.0] - <date>` section with real, non-empty content — consumed by Task 5's `extract-changelog.sh 0.1.0` (release notes) and by `check-release-invariants.sh v0.1.0` (which currently fails without it — verified directly: Chunk 1 left `CHANGELOG.md` with only an `## [Unreleased]` heading).

- [ ] **Step 1: Confirm the current gap**

Run: `scripts/check-release-invariants.sh v0.1.0`

Expected: fails with `CHANGELOG.md has no non-empty '## [0.1.0] - YYYY-MM-DD' section for tag v0.1.0` — confirming the real reason this task exists, not a hypothetical.

- [ ] **Step 2: Write the entry**

In `CHANGELOG.md`, change:

```markdown
# Changelog

## [Unreleased]
```

to (replace `<TODAY>` with the actual current date in `YYYY-MM-DD` format — run `date -u +%Y-%m-%d` and use that exact value, not a guessed or placeholder date):

```markdown
# Changelog

## [Unreleased]

## [0.1.0] - <TODAY>

### Added

- Capture core — the stream, Now, search (FTS5), merge, tags, projects.
- MCP server + CLI — `queue_take`/`queue_peek`, `capture_add`/`done`/`fail`/`handback`,
  `capture_search`; the `magpie` CLI (`add|list|search|done|drain|export|serve-mcp`).
- Screenshots + OCR — region capture, searchable text (macOS verified; Linux implemented,
  not yet run on real hardware — see this release's Linux hardware check in `RELEASING.md`,
  a later chunk).
- Prompt packs — `magpie pack add github:someone/agent-prompts`, fill-in-the-blank templates.
- Confidence-aware capture filing — the desktop toast proposes a destination project and
  commits it on a quick tap; never auto-files.
- Sessions, handback, and session digests — an MCP-connected agent's session is a persistent
  record; `capture_handback` gives it a third outcome beyond done/fail; ending a session
  writes a searchable summary into the stream.
- Capture list v2 — sections, soft-delete with real Undo and Recently Deleted, Markdown
  rendering, a context-menu interaction model, keyboard-first navigation, and remappable
  global hotkeys.
- The "3a canonical" redesign — two-theme Slate/Paper token layer, the main window rebuilt
  around a session strip and one overlay stack, hold-to-aim, and Across (⌘⌥K).
- In-app update checking and installation — background and manual checks, auto-download,
  and user-initiated install and relaunch; never installs or relaunches without you seeing
  what's in the update first.

### Known limitations

- Linux builds are CI-verified but not yet hand-verified on real hardware for this specific
  release (tracked as a one-time manual check before this tag is treated as the real first
  release — see `RELEASING.md`, a later chunk).
- macOS builds are unsigned and unnotarized in this release — the Apple Developer ID isn't
  set up yet. Only the Linux artifact is published for `v0.1.0`.
```

This is written straight from `ROADMAP.md`'s "Now" list (verified directly, not summarized from memory) plus the update-checking work from Chunks 2-3, honestly scoped to what's actually true of `v0.1.0` (Linux-only, unsigned macOS) rather than overclaiming.

- [ ] **Step 3: Verify**

Run: `scripts/check-release-invariants.sh v0.1.0`

Expected: still fails, but now *only* on the version-mismatch check (`Cargo.toml`'s version isn't `0.1.0` yet unless it already is — check `Cargo.toml:12`) — confirming the changelog gap specifically is closed. If `Cargo.toml`'s `[workspace.package] version` is already `"0.1.0"`, this command now passes entirely; either outcome is correct depending on that pre-existing value, just confirm the *changelog* failure message specifically is gone.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "Write the real 0.1.0 changelog entry"
```

---

### Task 3: `release.yml` — `guard` and `create-release` jobs

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `scripts/check-release-invariants.sh` (Chunk 1, tag-arg mode), `scripts/extract-changelog.sh` (Chunk 1).
- Produces: `guard` job outputs `tag`, `version`, `previous_tag`, `macos_enabled`, `matrix` — consumed by Task 4's `build`/`checksums-and-attest`/`publish` jobs (all four downstream jobs read `needs.guard.outputs.tag` rather than each re-deriving it). `create-release` job output `release_id` — consumed by Task 4's `build` job.

- [ ] **Step 1: Write the workflow header and `guard` job**

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:
    inputs:
      tag:
        description: "Tag to (re-)run the release pipeline for, e.g. v0.1.0"
        required: true

jobs:
  guard:
    runs-on: ubuntu-22.04
    permissions:
      contents: read
    outputs:
      tag: ${{ steps.tag.outputs.tag }}
      version: ${{ steps.compute.outputs.version }}
      previous_tag: ${{ steps.compute.outputs.previous_tag }}
      macos_enabled: ${{ steps.compute.outputs.macos_enabled }}
      matrix: ${{ steps.compute.outputs.matrix }}
    steps:
      - name: Resolve the tag
        id: tag
        run: echo "tag=${{ github.event.inputs.tag || github.ref_name }}" >> "$GITHUB_OUTPUT"

      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          ref: ${{ steps.tag.outputs.tag }}
          fetch-depth: 0

      - name: Check release invariants
        run: scripts/check-release-invariants.sh "${{ steps.tag.outputs.tag }}"

      # A tag pushed against a branch that never merged to main (or a stale
      # local branch) must never reach a real release -- this is the guard
      # against cutting a release from code nobody reviewed on the trunk.
      - name: Assert the tag's commit is on main
        run: |
          git fetch origin main
          if ! git merge-base --is-ancestor HEAD origin/main; then
            echo "::error::${{ steps.tag.outputs.tag }}'s commit is not on main -- refusing to release from an unmerged branch."
            exit 1
          fi

      - name: Compute version, previous tag, macOS gating, and build matrix
        id: compute
        run: |
          tag="${{ steps.tag.outputs.tag }}"
          version="${tag#v}"
          echo "version=$version" >> "$GITHUB_OUTPUT"

          # Most recent *other* tag, by creation date -- empty for the very
          # first release, which the create-release job's changelog-compare
          # line handles explicitly rather than producing a broken link.
          previous_tag=$(git tag --sort=-creatordate | grep -v "^${tag}$" | head -1 || true)
          echo "previous_tag=$previous_tag" >> "$GITHUB_OUTPUT"

          if [ "${{ vars.MACOS_RELEASE_ENABLED }}" = "true" ]; then
            echo "macos_enabled=true" >> "$GITHUB_OUTPUT"
            echo 'matrix=[{"os":"ubuntu-22.04","platform":"linux"},{"os":"macos-latest","platform":"macos"}]' >> "$GITHUB_OUTPUT"
          else
            echo "macos_enabled=false" >> "$GITHUB_OUTPUT"
            echo 'matrix=[{"os":"ubuntu-22.04","platform":"linux"}]' >> "$GITHUB_OUTPUT"
          fi
```

No `is_prerelease` output — this project has no prerelease-tag concept (see the master plan's grilling-pass decision); a `-rc`-suffixed tag simply fails `check-release-invariants.sh`'s ordinary version-match check above, and the pipeline stops right here.

`guard` is the only job that derives the tag from `github.event.inputs.tag || github.ref_name` — every other job below reads `needs.guard.outputs.tag` instead of re-deriving it, so there's exactly one place that logic lives.

- [ ] **Step 2: Add the `create-release` job**

Append to `.github/workflows/release.yml`:

```yaml

  create-release:
    needs: guard
    runs-on: ubuntu-22.04
    permissions:
      contents: write
    outputs:
      release_id: ${{ steps.create-or-reuse.outputs.release_id }}
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          ref: ${{ needs.guard.outputs.tag }}

      - name: Build the release body
        run: |
          tag="${{ needs.guard.outputs.tag }}"
          version="${{ needs.guard.outputs.version }}"
          previous_tag="${{ needs.guard.outputs.previous_tag }}"

          {
            scripts/extract-changelog.sh "$version"
            echo
            if [ -n "$previous_tag" ]; then
              echo "**Full Changelog**: https://github.com/${{ github.repository }}/compare/${previous_tag}...${tag}"
            fi
            if [ "${{ needs.guard.outputs.macos_enabled }}" != "true" ]; then
              echo
              echo "_macOS builds aren't attached to this release yet -- code signing isn't set up. Linux only for now._"
            fi
          } > /tmp/release-body.md

      # /tmp/release-body.md is passed directly to --notes-file below (same
      # job, same runner filesystem) -- no need to round-trip it through a
      # GITHUB_OUTPUT variable just to read it back one step later.

      # Self-healing for workflow_dispatch retries: a prior run's build job
      # can fail after this job already created the draft. None found ->
      # create fresh. Found and still a draft -> delete it first, then
      # create fresh (safe: a draft is never externally visible, and this
      # means build's asset uploads and checksums-and-attest's downloads
      # never need their own dedup/clobber logic). Found and already
      # published -> hard-fail; this case must never auto-delete, since a
      # published release may already be downloaded/depended on.
      - name: Create or reuse the draft release
        id: create-or-reuse
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          tag="${{ needs.guard.outputs.tag }}"
          existing_json=$(gh release view "$tag" --json databaseId,isDraft 2>/dev/null || true)

          if [ -n "$existing_json" ]; then
            is_draft=$(echo "$existing_json" | jq -r .isDraft)
            if [ "$is_draft" != "true" ]; then
              echo "::error::$tag was already released; re-running won't touch a published release -- cut a new patch version instead."
              exit 1
            fi
            gh release delete "$tag" --yes
          fi

          # Runs whether a stale draft was just deleted above or nothing
          # existed at all -- both cases start fresh from here. GitHub
          # Actions' default bash shell runs with `-e`, so a failing
          # `gh release create` already aborts this step; no manual `|| exit
          # 1` needed.
          gh release create "$tag" --draft --notes-file /tmp/release-body.md --title "$tag"

          release_id=$(gh release view "$tag" --json databaseId --jq .databaseId)
          echo "release_id=$release_id" >> "$GITHUB_OUTPUT"
```

`--draft` here is purely the technical guard against `/latest/` ever resolving a half-uploaded release, not a review pause — `publish` (Task 4) runs automatically once every prior job succeeds.

- [ ] **Step 3: Verify the YAML is well-formed**

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/release.yml")'` (or any YAML parser available locally) and confirm no parse error. This workflow can't be exercised for real yet (no `build` job exists, and it needs the real signing key) — this is a syntax-only check for this task.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "Add release.yml: guard and create-release jobs"
```

---

### Task 4: `release.yml` — `build`, `checksums-and-attest`, `publish` jobs

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `guard`'s `matrix`/`macos_enabled`/`version` outputs, `create-release`'s `release_id` output (Task 3).

- [ ] **Step 1: Add the `build` job**

In `.github/workflows/release.yml`, append after the `create-release` job (at the end of the file):

```yaml

  build:
    needs: [guard, create-release]
    strategy:
      fail-fast: false
      matrix:
        include: ${{ fromJson(needs.guard.outputs.matrix) }}
    runs-on: ${{ matrix.os }}
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
        with:
          ref: ${{ needs.guard.outputs.tag }}

      - name: Install Linux system dependencies
        if: matrix.platform == 'linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf \
            build-essential \
            curl \
            wget \
            file \
            libssl-dev \
            libgtk-3-dev

      - uses: dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4 # stable

      # ci.yml's build job never needs this -- it only ever builds for the
      # runner's native arch to verify the build works, not to produce a
      # universal binary. A real universal-apple-darwin build needs both
      # Darwin targets installed regardless of which one the runner's host
      # arch already has.
      - name: Add both macOS Rust targets
        if: matrix.platform == 'macos'
        run: |
          rustup target add x86_64-apple-darwin
          rustup target add aarch64-apple-darwin

      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2

      - uses: pnpm/action-setup@b906affcce14559ad1aafd4ab0e942779e9f58b1 # v4
        with:
          package_json_file: apps/desktop/package.json

      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: apps/desktop/pnpm-lock.yaml

      - name: Install frontend dependencies
        working-directory: apps/desktop
        run: pnpm install --frozen-lockfile

      - uses: tauri-apps/tauri-action@84b9d35b5fc46c1e45415bdb6144030364f7ebc5 # v0
        env:
          GITHUB_TOKEN: ${{ github.token }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          projectPath: apps/desktop
          releaseId: ${{ needs.create-release.outputs.release_id }}
          includeUpdaterJson: true
          args: ${{ matrix.platform == 'macos' && '--target universal-apple-darwin --config src-tauri/tauri.release.conf.json --locked' || '--config src-tauri/tauri.release.conf.json --locked' }}

      # Artifact integrity, not a GUI launch -- see the master plan's
      # grilling-pass decision on why a real headless launch was rejected
      # (magpie's tray/hotkey init have zero test coverage and need a real
      # D-Bus session, not just Xvfb; simulating one would be genuinely new,
      # untested CI infrastructure). This catches a corrupt/malformed
      # bundle cheaply, before checksums-and-attest ever sees it.
      - name: Verify bundle integrity (Linux)
        if: matrix.platform == 'linux'
        run: |
          set -e
          cd target/release/bundle
          appimage=$(find appimage -name '*.AppImage')
          "$appimage" --appimage-extract
          test -x squashfs-root/AppRun
          rm -rf squashfs-root
          deb=$(find deb -name '*.deb')
          dpkg-deb --contents "$deb" | grep -q 'usr/bin/magpie'

      - name: Verify bundle integrity (macOS)
        if: matrix.platform == 'macos'
        run: |
          set -e
          dmg=$(find target/universal-apple-darwin/release/bundle/dmg -name '*.dmg')
          mount_point=$(hdiutil attach "$dmg" -nobrowse -readonly | tail -1 | awk '{print $NF}')
          app=$(find "$mount_point" -maxdepth 1 -name '*.app')
          test -x "$app/Contents/MacOS/magpie"
          hdiutil detach "$mount_point"
          # codesign -dv --verify "$app" -- enabled once Phase 3 signing exists.
```

A failure here fails the `build` job, which blocks `publish` — the draft release the bad artifact was already uploaded to (`tauri-action` uploads as it builds) simply never flips to public.

- [ ] **Step 2: Add the `checksums-and-attest` job**

Append:

```yaml

  checksums-and-attest:
    needs: [guard, create-release, build]
    runs-on: ubuntu-22.04
    permissions:
      contents: write
      id-token: write
      attestations: write
    steps:
      - name: Download exactly what's attached to the draft
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release download "${{ needs.guard.outputs.tag }}" --dir /tmp/release-assets

      - name: Generate SHA256SUMS
        run: |
          cd /tmp/release-assets
          sha256sum * > SHA256SUMS

      - name: Upload SHA256SUMS
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload "${{ needs.guard.outputs.tag }}" /tmp/release-assets/SHA256SUMS

      - uses: actions/attest-build-provenance@e8998f949152b193b063cb0ec769d69d929409be # v2
        with:
          subject-path: "/tmp/release-assets/*"
```

`attest-build-provenance` also picks up `SHA256SUMS` itself in `subject-path`'s glob (uploaded in the previous step, but the glob only needs to match files present when this step runs — since `SHA256SUMS` was already uploaded and `gh release download` already ran before this point in the job, re-running `gh release download` isn't needed; the local `/tmp/release-assets/SHA256SUMS` file written in the previous step already satisfies the glob directly). Downloads exactly what's attached to the draft — not a `target/release/bundle` glob — so this attests what actually shipped.

- [ ] **Step 3: Add the `publish` job**

Append:

```yaml

  publish:
    needs: [guard, create-release, build, checksums-and-attest]
    runs-on: ubuntu-22.04
    permissions:
      contents: write
    steps:
      - name: Publish
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release edit "${{ needs.guard.outputs.tag }}" --repo "${{ github.repository }}" --draft=false
```

Runs automatically once every prior job succeeds. No manual trigger, no approval step.

- [ ] **Step 4: Verify the YAML is well-formed**

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/release.yml")'` and confirm no parse error. Also run `cargo build --workspace` and `pnpm --dir apps/desktop exec tsc --noEmit` to confirm nothing else in the repo broke (this task only touches the workflow file, so this should be a no-op check).

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "Add release.yml: build, checksums-and-attest, publish jobs"
```

---

### Task 5: Real end-to-end verification — the actual `v0.1.0` release

**Files:** none (verification + real secrets + a real tag).

This task requires the human partner's direct involvement at two points: providing the real signing key material (Step 1), and explicitly confirming before the tag is pushed (Step 4) — a pushed `v*` tag triggers a real, automated, public release with no further human checkpoint by design (see the master plan's "full automation" decision). Do not proceed past Step 3 without that confirmation.

- [ ] **Step 1: Phase 0 Step 1 — the real keypair (human action required)**

Ask the human partner to run, on their own machine (not delegated to an agent):

```
cd apps/desktop && pnpm tauri signer generate -w ~/.magpie/updater.key
```

They give the implementer the printed **public** key. They set the **private** key's contents as repo secret `TAURI_SIGNING_PRIVATE_KEY` and its passphrase as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — either themselves via the GitHub UI, or by asking for help running `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.magpie/updater.key` (a command that pipes the file directly to GitHub's API without the raw key ever appearing in any agent's context or chat transcript). Confirm both secrets exist (`gh secret list`, which shows names only, never values) before continuing. **Back the private key up offline** — there is no rotation path; losing it means no existing install will ever accept another update.

- [ ] **Step 2: Swap in the real pubkey**

In `apps/desktop/src-tauri/tauri.conf.json`, change:

```json
      "pubkey": "REPLACE_ME_BEFORE_FIRST_RELEASE"
```

to the real public key string the human partner provided, e.g.:

```json
      "pubkey": "<the real public key>"
```

Commit:

```bash
git add apps/desktop/src-tauri/tauri.conf.json
git commit -m "Swap placeholder updater pubkey for the real one"
```

- [ ] **Step 3: Push this branch and merge to `main`**

The `guard` job's "tag's commit is on main" check means the tag must point at a commit that's actually merged. Push this chunk's branch, open a PR, confirm `lint-and-test`/`build` pass on both platforms (matching Chunks 1 and 2's own definition of done), and merge it — following the same process already used for Chunks 1 and 2 in this session (ask before pushing, ask before merging).

- [ ] **Step 4: Cut the real `v0.1.0` tag — STOP and get explicit confirmation first**

This is the point of no easy return: pushing this tag triggers `release.yml` end to end, automatically, with no further human checkpoint, ending in a real published GitHub Release. Confirm explicitly with the human partner immediately before running this — do not treat this as routine "final verification" to run through without pausing.

Once confirmed, from `main` (after Step 3's merge, with `Cargo.toml`'s version already `0.1.0` — confirm via `grep version Cargo.toml`; if it isn't `0.1.0` yet, that's a real gap to resolve with the human partner before tagging, not something to silently work around):

```bash
git tag v0.1.0
git push origin v0.1.0
```

- [ ] **Step 5: Watch the real run**

Run: `gh run watch` (or `gh run list --workflow=release.yml` then `gh run watch <id>`).

Confirm: `guard` → `create-release` → `build` (Linux only — no macOS entry in the matrix, since `MACOS_RELEASE_ENABLED` is unset) → `checksums-and-attest` → `publish` all succeed in order. Confirm the release is live: `gh release view v0.1.0` shows `isDraft: false`. Confirm `latest.json` actually resolves: `curl -fsSL https://github.com/akshaykrishh/magpie/releases/latest/download/latest.json` returns real JSON, not a 404.

- [ ] **Step 6: Verify the in-app update path against the real release**

On a Linux machine (or the CI-built AppImage downloaded locally), with an old build still installed or an artificially old `update_last_checked_at` setting:

```bash
sqlite3 ~/.local/share/magpie/magpie.db "UPDATE settings SET value = '2020-01-01T00:00:00.000000000Z' WHERE key = 'update_next_check_at';"
```

Launch the app, click "Check for updates" in Settings → About (or wait for the background check). Confirm it finds `v0.1.0`, downloads, and "Install and relaunch" actually relaunches into the new version.

- [ ] **Step 7: The manual Linux hardware check — required for this tag specifically, not future ones**

Per the master plan's decision: `v0.1.0` is gated on someone launching the built AppImage or `.deb` on **real Linux hardware** (not CI) and confirming the tray icon, hotkey, and a capture all work. This is a written checklist item that belongs in `RELEASING.md` (a later chunk writes that file); for this task, just do the check itself and report the result — this is not something CI can enforce, and not required again for `v0.1.1` onward once Linux is a known-working platform.

- [ ] **Step 8: Verify checksums and attestation for real**

```bash
gh release download v0.1.0 --dir /tmp/v0.1.0-verify
cd /tmp/v0.1.0-verify
sha256sum -c SHA256SUMS
gh attestation verify magpie_0.1.0_amd64.AppImage --repo akshaykrishh/magpie
```

Expected: both succeed, confirming the checksums match and the attestation is real and verifiable — not just that the job reported success.

---

## Chunk Verification Summary

- `scripts/check-release-invariants.sh v0.1.0` passes locally after Task 2 (once `Cargo.toml`'s version is `0.1.0`).
- `.github/workflows/release.yml` is well-formed YAML after Tasks 3-4, verified as each half is written.
- `cargo build --workspace` and `pnpm --dir apps/desktop exec tsc --noEmit` stay clean throughout Tasks 1-4 (this chunk touches no application code, only config and CI).
- Task 5's real, live proof: a real `v0.1.0` tag, guard → create-release → build (Linux-only) → checksums-and-attest → publish all succeed automatically, `latest.json` resolves, an old install finds and installs the real update via the in-app flow, `sha256sum -c` and `gh attestation verify` both pass against the actual uploaded artifacts, and the manual real-Linux-hardware check confirms the app actually works outside CI.
- No behavior in Tasks 1-4 depends on the real signing key — matches the design doc's "this is the first point the real key actually has to exist" framing, i.e. only at Task 5, not before.
