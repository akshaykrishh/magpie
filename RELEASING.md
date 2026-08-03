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
