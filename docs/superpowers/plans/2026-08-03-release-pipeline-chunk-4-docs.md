# Release Pipeline Chunk 4 — Docs and Process Artifacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write the maintainer- and user-facing documentation that a real, published `v0.1.0` release now needs: finish `CHANGELOG.md`, write `RELEASING.md` and `SECURITY.md`, add the docs-site `updates.mdx` page, and bring `README.md`, `ROADMAP.md`, `docs/design.md`, `installation.mdx`, and `architecture.mdx` up to date with what's actually shipped.

**Architecture:** Pure documentation chunk — no application code or CI changes. Every claim in every file must match the real, currently-merged state of the repo (verified against `.github/workflows/release.yml`, `apps/desktop/src-tauri/updater.rs`, `apps/desktop/src/SettingsApp.tsx`, and `apps/desktop/src-tauri/tauri.conf.json` as they exist on `main` right now, post-Chunk-3), not the aspirational state the master plan described before Chunk 3 shipped.

**Tech Stack:** Markdown (root docs), MDX + Fumadocs frontmatter (`apps/docs/content/docs/*.mdx`), no build tooling required to verify — an MDX file only needs valid frontmatter and no broken relative links.

## Global Constraints

- **Every factual claim must be independently true against the current repo**, not copied from the master plan's older, pre-Chunk-3 framing. Two live discrepancies between the master plan's draft text and reality that every task must route around:
  - The master plan's SECURITY.md sketch says `codesign`/`spctl` on macOS — **macOS builds are not published at all yet** (`vars.MACOS_RELEASE_ENABLED` is unset, `guard`'s `matrix` output is Linux-only). Don't document a macOS verification step nobody can actually run today.
  - Steps 6–7 of Chunk 3 Task 5 (in-app update check against the real release, and the manual Linux hardware check) are **still open** — no Linux hardware access yet. Do not write anything claiming Linux has been hand-verified. `CHANGELOG.md`'s existing "Known limitations" section already states this accurately; match it, don't contradict it.
- **No formal prerelease/`-rc` tags** — explicitly out of scope. Reasoning that must appear in `RELEASING.md`: GitHub's `/releases/latest/` resolution excludes prereleases, so the updater could never deliver one; hand-authored changelogs and formal RC tags are an awkward pairing in practice. The actual early-access path is pointing someone at a `ci.yml` build artifact from any branch/commit.
- **SQLite migrations are forward-only** — downgrading after an update is unsupported. True today (see `crates/magpie-core/src/db.rs`'s `migrate()`), never written down until this chunk.
- **The signing-key trust boundary must be stated explicitly, not glossed over**: the private Ed25519 updater-signing key lives only in GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — confirmed via `gh secret list` against the real repo), the public key is committed in `apps/desktop/src-tauri/tauri.conf.json`'s `plugins.updater.pubkey` and baked into every binary at build time, and **there is no rotation path if the private key is compromised** — almost no project states this, and it's true of all of them.
- **Attestation is provenance, not bit-reproducibility.** Rust/Tauri builds are not byte-reproducible. Every place this chunk documents `gh attestation verify`, state plainly what it does and doesn't prove.
- **The Accessibility-grant reasoning in `docs/design.md`'s Signing passage is correct and load-bearing** — when that passage is rewritten to present tense, keep the actual reasoning sentence verbatim: macOS binds Accessibility grants to a binary's signature, so unsigned builds revoke the grant on every update, silently breaking one-key capture for the users who upgrade most.
- **`magpie` is the reserved product name; `desktop` is only the internal Cargo package name** (`apps/desktop/src-tauri/Cargo.toml`'s `[package] name`) — `tauri.conf.json`'s `mainBinaryName: "magpie"` override means every shipped binary is actually named `magpie`. Never write `usr/bin/desktop` or similar into user-facing docs.
- Match each file's existing voice: root `.md` files (`README.md`, `ROADMAP.md`) use terse, scannable prose with real bullet structure; `docs/design.md` is long-form reasoned prose; `apps/docs/content/docs/*.mdx` pages use second-person, task-oriented phrasing (see `installation.mdx`, `architecture.mdx` for the existing register).

---

### Task 1: Finish `CHANGELOG.md`

**Files:**
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: the explanatory header paragraph other tasks (`RELEASING.md`) reference when describing "why this file is hand-maintained."

The file already has a real `## [0.1.0] - 2026-08-03` section (written in Chunk 3) and a `## [Unreleased]` placeholder. This task adds only the explanatory header between the `# Changelog` title and `## [Unreleased]`, explaining the Keep a Changelog structure and why it's hand-maintained rather than auto-generated.

- [ ] **Step 1: Add the header paragraph**

Insert this text between line 1 (`# Changelog`) and the existing `## [Unreleased]` line:

```markdown
# Changelog

All notable changes to magpie are documented here, hand-written, in
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. This project
adheres to [Semantic Versioning](https://semver.org/) (see `RELEASING.md` for
the pre-1.0 caveat).

This file is hand-maintained, not auto-generated from commit messages or PR
titles. This maintainer's commit history is long-form reasoned prose, not
Conventional Commits — an auto-generator would flatten exactly what makes
those messages worth reading into a mechanical one-liner. One piece of
writing here serves three destinations: this file, the GitHub release body
(`scripts/extract-changelog.sh` slices out one version's section verbatim),
and the in-app "Release notes" link Settings → About shows for the installed
version.

## [Unreleased]

## [0.1.0] - 2026-08-03
```

Everything from `## [0.1.0]` onward is unchanged — only the header paragraph and blank lines above it are new.

- [ ] **Step 2: Verify**

```bash
head -20 CHANGELOG.md
```

Expected: the new header paragraph appears, followed immediately by the existing `## [Unreleased]` and `## [0.1.0] - 2026-08-03` sections with all their original content intact — `diff` the `## [0.1.0]` section specifically against git history to confirm nothing below the header changed:

```bash
git diff CHANGELOG.md | grep '^-' | grep -v '^---'
```

Expected: no output (the diff should be pure addition, zero deletions).

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: explain why CHANGELOG.md is hand-maintained"
```

---

### Task 2: Write `SECURITY.md`

**Files:**
- Create: `SECURITY.md`

**Interfaces:**
- Consumes: the real secret names from `gh secret list` (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), the real pubkey field location (`apps/desktop/src-tauri/tauri.conf.json`'s `plugins.updater.pubkey`).
- Produces: the verification commands (`sha256sum -c`, `gh attestation verify`) that `RELEASING.md` (Task 3) and `installation.mdx` (Task 6) both point readers at instead of duplicating.

- [ ] **Step 1: Write the file**

```markdown
# Security Policy

## Supported versions

Only the latest published release is supported. This is a pre-1.0,
solo-maintained project — there's no capacity to backport fixes to older
versions. Update to the latest release (Settings → About → Check for
updates, or download fresh from the
[releases page](https://github.com/akshaykrishh/magpie/releases)) before
reporting an issue.

## Reporting a vulnerability

Use GitHub's
[private vulnerability reporting](https://github.com/akshaykrishh/magpie/security/advisories/new)
for this repository — it opens a private conversation the maintainer can
see, without disclosing details publicly before a fix ships. If that's not
available for any reason, email akshaykrishnakanth@gmail.com.

This is a solo-maintained project. Expect an initial response within a
week, not within a business day — but a real security report will get
prioritized over everything else in the queue.

## Scope

magpie is a local-only desktop app: no server, no accounts, no telemetry.
The interesting attack surfaces are narrower than a typical web app's:

- **The MCP lease/audit boundary** — `queue_take`'s `BEGIN IMMEDIATE`
  transaction and the lease lifecycle (see
  [MCP integration](https://akshaykrishh.github.io/magpie/docs/mcp)) are
  what stop two agent sessions from racing on the same queue item. A bug
  here is a correctness *and* security question, since anything with MCP
  access can read and write the full capture stream.
- **The update trust chain** — see below.
- **Local data at rest** — the SQLite database is deliberately unencrypted
  and lives at a documented, conventional path (see
  [Architecture](https://akshaykrishh.github.io/magpie/docs/architecture)).
  This is a stated design choice (own-your-data through transparency, not
  through weaker storage), not an oversight — don't report "the database
  isn't encrypted" as a finding.

Out of scope: anything requiring physical access to an already-unlocked
machine, and social engineering.

## The update trust chain

Every release is signed with an Ed25519 keypair (via `tauri-plugin-updater`,
minisign-compatible) before it's ever downloaded by a running app:

- The **private key** exists in exactly one place: this repo's GitHub
  Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`,
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), used only inside `release.yml`'s
  `build` job at release time, plus one offline backup outside GitHub. It
  is never checked into git and never leaves those two places.
- The **public key** is committed in
  `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) and
  compiled directly into every shipped binary. A running app only ever
  installs an update whose signature verifies against the pubkey baked in
  at *its own* build time — an attacker controlling the download host
  without the private key cannot produce a payload the app will accept.
- **There is no rotation path if the private key is compromised.** This is
  true of nearly every project's updater signing key, and nearly none of
  them say so. If it ever happens, every existing install stops trusting
  new releases until it's manually reinstalled from a fresh download with
  a new pubkey baked in — there's no in-place recovery.

## Verifying a download

Every release includes `SHA256SUMS` and a
[build provenance attestation](https://github.com/akshaykrishh/magpie/attestations).
To verify an artifact after downloading it:

```bash
gh release download vX.Y.Z --repo akshaykrishh/magpie
sha256sum -c SHA256SUMS
gh attestation verify magpie_X.Y.Z_amd64.AppImage --repo akshaykrishh/magpie
```

Both should succeed. `sha256sum -c` confirms the file wasn't corrupted or
tampered with in transit; `gh attestation verify` confirms it was actually
built by this repo's `release.yml` workflow via GitHub's OIDC-backed
attestation, not substituted afterward.

**What this does *not* prove:** attestation is provenance, not
bit-reproducibility. It confirms *which workflow run* produced the
artifact, not that you (or anyone else) could rebuild an identical byte-
for-byte copy from source — Rust/Tauri builds aren't reproducible builds.
Don't over-claim what a passing `gh attestation verify` means.

macOS builds aren't published yet (see
[Architecture](https://akshaykrishh.github.io/magpie/docs/architecture)) —
there's no `codesign`/`spctl` verification step to document until that
changes.
```

- [ ] **Step 2: Verify**

```bash
gh secret list
```

Expected: exactly `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — confirms the secret names in the file match reality, not the master plan's earlier draft naming.

```bash
grep -n "pubkey" apps/desktop/src-tauri/tauri.conf.json
```

Expected: confirms `plugins.updater.pubkey` is the real field path referenced.

- [ ] **Step 3: Commit**

```bash
git add SECURITY.md
git commit -m "docs: add SECURITY.md"
```

---

### Task 3: Write `RELEASING.md`

**Files:**
- Create: `RELEASING.md`

**Interfaces:**
- Consumes: the exact job names and behavior of `.github/workflows/release.yml` (5 jobs: `guard`, `create-release`, `build`, `checksums-and-attest`, `publish`), the exact secret names (Task 2), the exact `workflow_dispatch` input shape (`tag`, required string).
- Produces: nothing consumed by later tasks, but its "what leaves your machine" framing must stay consistent with Task 4's `updates.mdx`.

This is maintainer-facing, not linked from the docs site — it exists specifically so the next release doesn't require re-deriving how the pipeline works from `release.yml`'s source.

- [ ] **Step 1: Write the file**

```markdown
# Releasing magpie

Maintainer-facing. Not linked from the docs site — this exists so cutting a
release doesn't require re-reading `.github/workflows/release.yml` from
scratch every time.

## One-time setup

Already done for this repo; documented here so it's not lost if it ever
needs redoing (a new machine, a compromised key, a fork).

1. **Generate the updater signing keypair:**

   ```bash
   pnpm --dir apps/desktop exec tauri signer generate -w ~/.magpie/updater.key
   ```

   This writes `~/.magpie/updater.key` (private) and
   `~/.magpie/updater.key.pub` (public).

2. **Set the two GitHub Actions secrets** (Settings → Secrets and
   variables → Actions):

   | Secret | Value |
   |---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.magpie/updater.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password chosen when generating the key |

   ```bash
   gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.magpie/updater.key
   ```

   The password has no file to read from — set it directly via the GitHub
   UI or `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD` typed
   interactively, never pasted into a chat or agent context.

3. **Paste the public key into `tauri.conf.json`.** `~/.magpie/updater.key.pub`'s
   content, printed with `cat`, is *already* correctly encoded for
   `plugins.updater.pubkey` — do not re-encode it with `base64` again;
   that produces a second, wrong layer of encoding.

4. **Back up the private key offline**, outside GitHub, somewhere durable.
   There is no rotation path (see `SECURITY.md`) — losing this key means
   every future release needs a new keypair, and every existing install
   needs a fresh download with the new pubkey to ever trust an update
   again.

5. *(Deferred, Phase 3 — not needed for Linux-only releases)* Apple
   Developer Program enrollment, a Developer ID Application certificate
   exported as `.p12`, and an App Store Connect API key, once macOS
   releases are enabled.

## Versioning policy

SemVer. Pre-1.0, a breaking change bumps the minor version (`0.x.0`), not
the patch — there's no `1.0` stability contract yet to protect with a major
bump.

**No formal prerelease/`-rc` tags.** Deliberately out of scope, not an
oversight:

- GitHub's `/releases/latest/` resolution excludes prereleases, so the
  in-app updater (which always points at `/releases/latest/download/latest.json`)
  could never deliver one anyway.
- Hand-authored changelogs (see `CHANGELOG.md`'s header) and formal RC tags
  are an awkward pairing in practice — an RC's changelog entry would need
  writing and then rewriting once the real tag cuts.

The actual early-access path: point someone at a `ci.yml` build artifact
from any branch or commit (see `installation.mdx`'s "Development builds"
section) — unsigned, not through the updater, but available from any
commit without waiting for a tag.

**SQLite migrations are forward-only.** Downgrading the app after an
update is unsupported — there's no down-migration path in
`crates/magpie-core/src/db.rs`. If a release ships a bad migration, the fix
is a forward-fixing patch release, not telling anyone to roll back.

## Cutting a release

1. Bump the version in `Cargo.toml`'s workspace `[workspace.package]`
   `version` field — this is the single source of truth; `tauri.conf.json`
   deliberately has no `version` field of its own (see
   `scripts/check-release-invariants.sh`).
2. Add a real `## [X.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md`, above
   `## [Unreleased]`.
3. Run `scripts/check-release-invariants.sh` locally (no tag arg — this is
   also a `ci.yml` step on every push) to confirm the version and
   changelog are in sync before tagging.
4. **For `v0.1.0` only** (see below): run the manual Linux hardware check
   *before* tagging, since nothing automated can catch a tray/hotkey/portal
   regression.
5. Tag and push:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   This triggers `release.yml` automatically. No further manual step —
   `publish` flips the release live with no pause for confirmation, so
   don't push the tag until you mean it.
6. Watch the run: `gh run list --workflow=release.yml --limit 1`, then
   `gh run watch <id>`.
7. Once `publish` succeeds, confirm the release is actually live:

   ```bash
   gh release view vX.Y.Z --json isDraft   # expect false
   curl -fsSL https://github.com/akshaykrishh/magpie/releases/latest/download/latest.json
   ```

### The manual Linux hardware check — required for `v0.1.0` only

Per the original release-pipeline design decision: CI can prove the Linux
bundle is *well-formed* (`verify-bundle` extracts the AppImage and checks
the `.deb`'s contents), but nothing in CI launches the actual app — there's
no real D-Bus session or window manager on a CI runner, and simulating one
was deliberately rejected as new, untested CI infrastructure rather than a
verification step. So for the very first Linux release, a human has to
actually run it:

1. Download the real `.deb` or `.AppImage` from the `v0.1.0` release onto
   real Linux hardware (not a CI runner, not headless).
2. Launch it. Confirm the tray icon appears, the global capture hotkey
   works, and a real screenshot capture (region select + OCR) succeeds.
3. Also exercise the in-app update path once a newer version exists: age
   `update_next_check_at` artificially —

   ```bash
   sqlite3 ~/.local/share/magpie/magpie.db \
     "UPDATE settings SET value = '2020-01-01T00:00:00.000000000Z' \
      WHERE key = 'update_next_check_at';"
   ```

   — launch the app, use Settings → About → "Check for updates," and
   confirm it finds the release, downloads, and "Install and relaunch"
   actually relaunches into the new version.

This is a one-time gate for `v0.1.0`, not a checklist item for every
future release — once Linux is a known-working platform, `v0.1.1` onward
skips straight to tagging.

## What each job does

`release.yml` has 5 jobs, always in this order:

- **`guard`** — resolves the tag (from the tag push, or from the
  `workflow_dispatch` `tag` input for re-runs), runs
  `check-release-invariants.sh`, asserts the tagged commit is actually an
  ancestor of `main` (refuses to release from an unmerged branch), and
  computes the build matrix — Linux-only unless the `MACOS_RELEASE_ENABLED`
  repo variable is `true`.
- **`create-release`** — builds the release body from
  `extract-changelog.sh`, then creates a draft GitHub release. Self-healing:
  if a draft already exists for this tag (a prior run's `build` job failed
  after this step), it deletes and recreates it. If a *published* release
  already exists for this tag, it hard-fails instead — this job will never
  touch a release someone might already be depending on.
- **`build`** — one job per matrix entry (Linux always; macOS once
  enabled). Compiles, bundles, and signs with the real updater key,
  attaches artifacts to the draft, then runs a bundle-integrity check
  (extracts the AppImage, inspects the `.deb`'s contents for
  `usr/bin/magpie`).
- **`checksums-and-attest`** — downloads everything attached to the draft,
  generates `SHA256SUMS`, uploads it, then runs
  `actions/attest-build-provenance` over every asset.
- **`publish`** — flips the draft to a published release. This is the only
  job with no further gate after it; once it runs, the release is public.

## Re-running via `workflow_dispatch`

If `build` (or any later job) fails, fix the underlying issue, merge the
fix to `main`, then re-run without moving the tag:

```bash
gh workflow run release.yml -f tag=vX.Y.Z
```

**Caveat that matters:** `workflow_dispatch` controls which ref the
*workflow orchestration* runs from (always wherever you dispatch it from —
typically `main`), but every job's `actions/checkout` step still checks out
`ref: <the tag>` for the actual source tree being built. A fix that only
touches `.github/workflows/release.yml` itself takes effect immediately on
re-run. A fix that touches application source (`tauri.conf.json`,
`Cargo.toml`, anything under `crates/` or `apps/`) does **not** take effect
until the tag itself is moved to a commit that includes it:

```bash
git tag -d vX.Y.Z
git push origin --delete vX.Y.Z
git tag vX.Y.Z origin/main
git push origin vX.Y.Z
```

Safe to do as long as the tag has never produced a *published* release yet
(check `gh release view vX.Y.Z --json isDraft` first) — `create-release`'s
self-healing means a still-draft release costs nothing to blow away and
recreate.

## Rollback

Two levers, both reuse GitHub's own release flags rather than anything
custom:

- **`gh release edit vX.Y.Z --prerelease`** on an already-published
  release — `/releases/latest/` immediately falls back to the previous
  stable release, so the updater stops offering the bad version, without
  deleting anything. (Unrelated to the dropped `-rc`-tag idea above; this
  just repurposes GitHub's existing prerelease flag as an emergency lever,
  not a planned release stage.)
- Re-draft: `gh release edit vX.Y.Z --draft` has the same immediate
  `/releases/latest/` fallback effect.

Either way, ship a real patch release with the fix as soon as possible —
these are stop-the-bleeding levers, not a substitute for fixing forward.

## Known, accepted gaps

Stated plainly rather than silently assumed as covered:

- Past `v0.1.0`, **nothing automated confirms a Linux build actually
  launches** (tray init, hotkey registration). `verify-bundle` only proves
  the package is well-formed, not that it runs. A real headless-launch
  smoke test is a candidate future follow-up, not something this pipeline
  claims to cover.
- **macOS never gets even a manual launch check** until Phase 3 (code
  signing) lands — there's no macOS artifact published to check today.
```

- [ ] **Step 2: Verify**

```bash
grep -n "^jobs:" -A2 .github/workflows/release.yml
awk '/^  [a-z-]+:$/' .github/workflows/release.yml
```

Expected: exactly `guard:`, `create-release:`, `build:`, `checksums-and-attest:`, `publish:` — confirms the "What each job does" section names and orders match the real file exactly.

```bash
grep -n "workflow_dispatch" -A4 .github/workflows/release.yml
```

Expected: confirms the `tag` input name and `required: true` match what's documented.

- [ ] **Step 3: Commit**

```bash
git add RELEASING.md
git commit -m "docs: add RELEASING.md"
```

---

### Task 4: Add the docs-site `updates.mdx` page

**Files:**
- Create: `apps/docs/content/docs/updates.mdx`
- Modify: `apps/docs/content/docs/meta.json`

**Interfaces:**
- Consumes: the real update-check behavior from `apps/desktop/src-tauri/updater.rs` (`CHECK_INTERVAL_HOURS = 6`, `update_next_check_at` persisted setting, `auto_check_on` gate) and the real Settings UI from `apps/desktop/src/SettingsApp.tsx` ("Automatically check for updates" checkbox, "Check for updates" button, "Install and relaunch" button, "Release notes" link).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Add "updates" to `meta.json`'s page list, right after "installation"**

Current `apps/docs/content/docs/meta.json`:

```json
{
  "title": "magpie",
  "pages": [
    "index",
    "installation",
    "quick-start",
    "concepts",
    "cli",
    "mcp",
    "architecture",
    "schema",
    "contributing"
  ]
}
```

New:

```json
{
  "title": "magpie",
  "pages": [
    "index",
    "installation",
    "updates",
    "quick-start",
    "concepts",
    "cli",
    "mcp",
    "architecture",
    "schema",
    "contributing"
  ]
}
```

- [ ] **Step 2: Write `apps/docs/content/docs/updates.mdx`**

```mdx
---
title: Updates
description: What the in-app updater checks, what it sends, and how to turn it off.
---

## What leaves your machine

The update check is the *only* network request magpie makes on its own.
It's a plain HTTPS `GET` for one static JSON file —
`https://github.com/akshaykrishh/magpie/releases/latest/download/latest.json`
— with no query parameters, no device ID, no telemetry, and no analytics.
There's nothing else to opt out of, because there's nothing else magpie
sends.

If you'd rather it never even makes that request, uncheck "Automatically
check for updates" in Settings → About. You can still check manually
whenever you want with the "Check for updates" button there — the only
difference is whether it happens on its own every 6 hours in the
background.

## How checking works

- A background check runs at most once every 6 hours, tracked by a
  persisted `update_next_check_at` timestamp — not a fixed in-process
  timer, so it survives the app being closed and reopened.
- A found update never installs itself. It shows up as "Update available"
  in Settings → About (and as an "Update to X.Y.Z" item in the tray menu),
  with a "Skip this version" option if you'd rather not be reminded about
  that particular release.
- Nothing downloads until you either click through from that "available"
  state or a background download quietly finishes on its own — either
  way, installing is always a separate, explicit "Install and relaunch"
  click. magpie never relaunches itself without you choosing to.
- "Release notes" next to the version number links out to that version's
  real GitHub release page — the same page `RELEASING.md`'s changelog
  process writes.

## Versioning, in user terms

magpie follows [Semantic Versioning](https://semver.org/), with one
pre-1.0 caveat: before `1.0`, a breaking change bumps the *minor* version
(`0.4.0` → `0.5.0`) instead of waiting for a major version that doesn't
exist yet. There's no way to opt into early-access or release-candidate
builds through the updater — every release is either the latest stable
version, or not offered yet. (Anyone who wants a build ahead of a tagged
release can grab one from CI directly — see
[Installation](/docs/installation)'s "Development builds" section — but
that path is unsigned and doesn't self-update.)

## Why installs never relaunch on their own, and why `.deb` installs don't self-update

Every install requires an explicit "Install and relaunch" click — magpie
never decides on its own that now is a good time to restart and lose
whatever state you had open.

The `.deb` package specifically has no self-update path at all: unlike the
`.AppImage` (which the in-app updater replaces in place), a `.deb`-installed
binary is owned by `dpkg`, and only `dpkg`/`apt` — not the app itself — can
replace a file it manages. If you installed via `.deb`, the in-app updater
will still tell you a new version exists, but you'll need to download and
reinstall the new `.deb` by hand.
```

- [ ] **Step 3: Verify**

```bash
python3 -c "
content = open('apps/docs/content/docs/updates.mdx').read()
assert content.startswith('---\n'), 'missing frontmatter start'
end = content.index('---', 4)
frontmatter = content[4:end]
assert 'title:' in frontmatter and 'description:' in frontmatter, 'missing required frontmatter fields'
print('frontmatter OK')
"
python3 -c "import json; d = json.load(open('apps/docs/content/docs/meta.json')); assert d['pages'].index('updates') == d['pages'].index('installation') + 1; print('meta.json OK')"
grep -n "CHECK_INTERVAL_HOURS" apps/desktop/src-tauri/src/updater.rs
```

Expected: all three succeed; the last command confirms `CHECK_INTERVAL_HOURS = 6` in the real source, matching the "every 6 hours" claim in the page.

- [ ] **Step 4: Commit**

```bash
git add apps/docs/content/docs/updates.mdx apps/docs/content/docs/meta.json
git commit -m "docs: add the updates.mdx docs-site page"
```

---

### Task 5: Update `README.md` and `docs/design.md`'s Signing passage

**Files:**
- Modify: `README.md`
- Modify: `docs/design.md` (Signing passage, currently lines 422-426)

**Interfaces:**
- Consumes: nothing from other tasks (independent of Tasks 1-4's new files, though it references `SECURITY.md` by name — the reference is valid once Task 2 has merged, but this task's own diff doesn't depend on Task 2's content).

- [ ] **Step 1: Replace `README.md`'s status paragraph**

Current (`README.md` lines 7-11):

```markdown
**Status: early development.** The capture core, MCP server, and CLI work end-to-end on macOS.
On Linux, the full test suite runs and passes in CI on every push, but nobody has yet run the
actual desktop app on a real Linux machine — that first-hand verification is still outstanding.
No signed releases yet; CI produces unsigned build artifacts on every push (see Development
below to build from source in the meantime).
```

New:

```markdown
**Status: early development.** The capture core, MCP server, and CLI work end-to-end on macOS.
[Signed Linux releases](https://github.com/akshaykrishh/magpie/releases) are available now
(`.deb` and `.AppImage`, in-app auto-updating) — macOS releases are pending code signing, see
below. The full test suite runs and passes in CI on every push, but nobody has yet run the
actual desktop app on real Linux hardware — that first-hand verification is still outstanding;
see [SECURITY.md](SECURITY.md) for verifying a download in the meantime.
```

- [ ] **Step 2: Update the Roadmap section's packaging line**

Current (`README.md` line 30):

```markdown
work on macOS; Linux has the code but not yet a first-hand run on real hardware. Packaging ships
unsigned installers from CI; real code signing is still a manual step.
```

New:

```markdown
work on macOS; Linux has the code but not yet a first-hand run on real hardware. Linux releases
are signed and auto-update; macOS code signing (needed before a Homebrew cask) is still a manual
step.
```

- [ ] **Step 3: Rewrite `docs/design.md`'s Signing passage to present tense**

Current (`docs/design.md` lines 422-426):

```markdown
**Signing:** develop with a stable self-signed identity so your own Accessibility grant survives
rebuilds. Buy the Apple Developer ID ($99/yr) and wire notarization into CI when cutting `v0.1`.
The reason isn't the scary dialog — macOS binds Accessibility grants to a signature, so unsigned
builds revoke the grant on **every update**, silently disabling one-key capture for the users who
upgrade most. Linux needs none of this.
```

New — the Accessibility-grant reasoning sentence is kept verbatim, only the surrounding tense and status change:

```markdown
**Signing:** development happens with a stable self-signed identity so a local Accessibility
grant survives rebuilds. Linux releases are signed with a real Ed25519 updater key today
(`.github/workflows/release.yml`'s `build` job, gated on nothing) and auto-update in place. macOS
releases are wired into the same pipeline but gated behind the `MACOS_RELEASE_ENABLED` repo
variable, unset until the Apple Developer ID ($99/yr) is purchased and notarization is wired in.
The reason isn't the scary dialog — macOS binds Accessibility grants to a signature, so unsigned
builds revoke the grant on **every update**, silently disabling one-key capture for the users who
upgrade most. Linux needs none of this.
```

- [ ] **Step 4: Verify**

```bash
grep -n "No signed releases yet\|unsigned build artifacts" README.md
```

Expected: no matches — confirms the stale claim is fully gone, not just softened.

```bash
grep -n "MACOS_RELEASE_ENABLED" docs/design.md
```

Expected: one match, confirming the new Signing passage names the real gating variable used in `release.yml`.

```bash
git diff docs/design.md | grep -c "binds Accessibility grants to a signature, so unsigned"
```

Expected: `1` — confirms the load-bearing reasoning sentence survived the rewrite verbatim (allowing for the surrounding line wrap).

- [ ] **Step 5: Commit**

```bash
git add README.md docs/design.md
git commit -m "docs: update README and design.md's Signing passage for the real v0.1.0 release"
```

---

### Task 6: Rewrite `apps/docs/content/docs/installation.mdx`

**Files:**
- Modify: `apps/docs/content/docs/installation.mdx` (near-total rewrite)

**Interfaces:**
- Consumes: `SECURITY.md`'s verification commands (Task 2) by reference/link — this task's content should link to `/docs/updates` (Task 4) too, since both land in the same chunk.

- [ ] **Step 1: Rewrite the file**

```mdx
---
title: Installation
description: Download a signed release, grab an unsigned CI build, or build from source.
---

## Download

**Linux** — signed, auto-updating (see [Updates](/docs/updates)). Grab the
`.AppImage` or `.deb` from the
[latest release](https://github.com/akshaykrishh/magpie/releases/latest)
(asset filenames are versioned, e.g. `magpie_0.1.0_amd64.AppImage`, so
there's no evergreen direct-download link to give here):

- **`.AppImage`** — self-updates in place through the app; no install
  step, just run it.
- **`.deb`** — installs system-wide via `dpkg`/`apt`, but **does not
  self-update** (see [Updates](/docs/updates)) — reinstall a new `.deb` by
  hand for each release.

**macOS** — not published yet. Code signing and notarization aren't wired
up (see [Architecture](/docs/architecture)); build from source below in
the meantime, or watch [ROADMAP.md](https://github.com/akshaykrishh/magpie/blob/main/ROADMAP.md)
for when a signed `.dmg` and Homebrew cask land.

## Verify

Every release includes `SHA256SUMS` and a build provenance attestation:

```bash
gh release download vX.Y.Z --repo akshaykrishh/magpie
sha256sum -c SHA256SUMS
gh attestation verify magpie_X.Y.Z_amd64.AppImage --repo akshaykrishh/magpie
```

See [`SECURITY.md`](https://github.com/akshaykrishh/magpie/blob/main/SECURITY.md)
for what each check does and doesn't prove, and how the update signing key
itself is protected.

## Build from source

Requires [Rust](https://www.rust-lang.org/tools/install) (stable) and
[pnpm](https://pnpm.io/installation). macOS builds also require Xcode Command Line Tools.

```bash
git clone https://github.com/akshaykrishh/magpie.git
cd magpie/apps/desktop
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` compiles the Rust core and launches the app with hot reload on the frontend.
For a release build (produces a `.dmg` on macOS, or a `.deb`/`.AppImage` on Linux):

```bash
pnpm tauri build
```

The bundle lands in `target/release/bundle/` at the repo root -- this is a Cargo workspace, so
every crate (including the desktop app) shares one `target/` directory rather than a per-app one.

## Development builds

Every push to `main` builds and bundles the app on both macOS and Ubuntu runners. Unsigned
artifacts (`.dmg`, `.deb`, `.AppImage`) are attached to each
[workflow run](https://github.com/akshaykrishh/magpie/actions/workflows/ci.yml) -- useful for
trying an unreleased commit without a Rust toolchain. Two things to know before using one:

- These builds **don't self-update** through the in-app updater — they're not part of the signed
  release pipeline at all, so you'd need to grab a new one by hand each time.
- On macOS specifically, rebuilding and reinstalling one of these **revokes your Accessibility
  grant on every rebuild** (see [Architecture](/docs/architecture)'s Signing section) — one-key
  capture will silently stop working until you re-grant it. This is a real cost of using an
  unsigned build, not a hypothetical one.

## The CLI

The CLI (`magpie`) and MCP server binary both build from the same workspace:

```bash
cargo build --release -p magpie-cli
# binary at target/release/magpie
```

See [CLI reference](/docs/cli) for every subcommand, and [MCP integration](/docs/mcp) for wiring
`magpie serve-mcp` into Claude Code or Cursor.
```

- [ ] **Step 2: Verify**

```bash
python3 -c "
content = open('apps/docs/content/docs/installation.mdx').read()
assert content.startswith('---\n')
end = content.index('---', 4)
fm = content[4:end]
assert 'title:' in fm and 'description:' in fm
print('frontmatter OK')
"
grep -c "No signed releases yet\|real code signing needs an Apple Developer ID, which is a" apps/docs/content/docs/installation.mdx
```

Expected: frontmatter OK, and the second grep returns `0` — confirms the stale "no signed releases" opening line is fully replaced, not left alongside the new Download section.

```bash
grep -n "/docs/updates" apps/docs/content/docs/installation.mdx
```

Expected: at least 2 matches — confirms the cross-links to Task 4's new page are actually present.

- [ ] **Step 3: Commit**

```bash
git add apps/docs/content/docs/installation.mdx
git commit -m "docs: rewrite installation.mdx for the real v0.1.0 release"
```

---

### Task 7: Update `ROADMAP.md` and `apps/docs/content/docs/architecture.mdx`

**Files:**
- Modify: `ROADMAP.md`
- Modify: `apps/docs/content/docs/architecture.mdx`

**Interfaces:**
- Consumes: nothing from other tasks — independent, small, doc-consistency edits.

- [ ] **Step 1: Narrow `ROADMAP.md`'s "Code signing and notarization" bullet to what's actually left**

Current (`ROADMAP.md` lines 48-51):

```markdown
- **Code signing and notarization** — CI ships unsigned artifacts today.
  Needed before a real v1: unsigned builds revoke macOS Accessibility
  grants on every update, silently breaking one-key capture for anyone who
  upgrades.
```

New:

```markdown
- **macOS code signing and notarization** — Linux releases are signed and
  auto-update today; macOS is gated behind purchasing an Apple Developer
  ID ($99/yr) and wiring notarization into `release.yml`'s already-built
  macOS job. Needed before a real v1: unsigned builds revoke macOS
  Accessibility grants on every update, silently breaking one-key capture
  for anyone who upgrades.
```

- [ ] **Step 2: Add a Now bullet for the releases/updater pipeline**

Insert as a new bullet in the `## Now` section, immediately after the
"The '3a canonical' redesign" bullet (`ROADMAP.md` line 36) and before
`## Next`:

```markdown
- **Signed releases and in-app updates** — `v0.1.0` onward ships signed,
  checksummed, attested Linux `.deb`/`.AppImage` builds via a tag-triggered
  pipeline; the app checks for, downloads, and installs updates on its
  own with your explicit confirmation before anything installs. See
  [RELEASING.md](RELEASING.md) for the process and
  [docs/design.md](docs/design.md) for the signing-key trust boundary.
```

- [ ] **Step 3: Update `architecture.mdx`'s verification table**

Current (`apps/docs/content/docs/architecture.mdx` line 64):

```markdown
| Code signing / notarization | **Not done.** Needs an Apple Developer ID, which is a manual step outside CI -- see below. |
```

New — split into three honest rows instead of one:

```markdown
| Updater signing | **Done.** Every release is signed with a real Ed25519 key; a running app only installs an update whose signature verifies against its own baked-in public key -- see [Updates](/docs/updates) and [SECURITY.md](https://github.com/akshaykrishh/magpie/blob/main/SECURITY.md). |
| Linux release build | **Done, CI-verified only.** `release.yml`'s `build` job compiles, bundles, and checks the artifact's contents on every tag push -- but nobody has run the actual app on real Linux hardware yet (see [Installation](/docs/installation)). |
| macOS code signing / notarization | **Not done.** Needs an Apple Developer ID, which is a manual step outside CI -- see below. |
```

- [ ] **Step 4: Verify**

```bash
grep -n "unsigned artifacts today" ROADMAP.md
```

Expected: no match — confirms the stale framing is gone.

```bash
grep -c "^| .* | " apps/docs/content/docs/architecture.mdx
```

Expected: one more row than before the edit (the original single "Code signing / notarization" row became three) — sanity-check by comparing against `git show HEAD:apps/docs/content/docs/architecture.mdx | grep -c "^| .* | "` before this task's commit.

- [ ] **Step 5: Commit**

```bash
git add ROADMAP.md apps/docs/content/docs/architecture.mdx
git commit -m "docs: update ROADMAP and architecture.mdx for the real v0.1.0 release"
```

---

## Chunk Verification Summary

- Every file this chunk touches or creates is prose/config only — no `cargo build`, `cargo test`,
  or `pnpm exec tsc --noEmit` regression is possible, but run them anyway once at the end of the
  chunk to confirm no stray edit leaked outside docs:

  ```bash
  cargo build --workspace
  pnpm --dir apps/desktop exec tsc --noEmit
  scripts/check-release-invariants.sh
  ```

- `git grep -n "usr/bin/desktop\|No signed releases yet\|unsigned build artifacts\|Code signing / notarization.*Not done"` across the whole repo after all 7 tasks — expect **zero matches**, confirming every stale claim this chunk was meant to fix is actually gone, not just added-alongside.
- `python3 -c "import json; json.load(open('apps/docs/content/docs/meta.json'))"` — valid JSON.
- Every new/modified `.mdx` file starts with valid `---`-delimited frontmatter containing `title:` and `description:`.
- `RELEASING.md`'s job list (`guard`, `create-release`, `build`, `checksums-and-attest`, `publish`) matches `awk '/^  [a-z-]+:$/' .github/workflows/release.yml` exactly.
- `SECURITY.md`'s secret names match `gh secret list` exactly.
