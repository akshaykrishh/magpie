## What does this change, and why?

## How was this tested?

## Checklist

- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass locally
- [ ] `pnpm exec tsc --noEmit && pnpm run build` pass locally (from `apps/desktop`)
- [ ] Touches `crates/magpie-capture`? Tested against the real running app, not just unit tests
- [ ] User-facing change? Added an entry to `CHANGELOG.md`'s `[Unreleased]` section
