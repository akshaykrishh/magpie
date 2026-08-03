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
