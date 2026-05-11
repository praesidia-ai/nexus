<!-- Thanks for contributing to Nexus! Please fill in the sections below. -->

## Summary

<!-- 1–3 sentences: what does this PR change and why? -->

## Type of change

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (API / schema / CLI flag changes)
- [ ] Refactor / internal cleanup
- [ ] Docs only
- [ ] CI / build / tooling

## Related issues

<!-- e.g. Closes #123, Refs #456 -->

## Checklist

- [ ] `cargo build` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] Frontend changes: `cd web && npm run build` passes
- [ ] Added/updated tests for the change
- [ ] Updated `CHANGELOG.md` under `## [Unreleased]`
- [ ] Updated user-facing docs (README / handler docs / SDK docs) where relevant
- [ ] No secrets, API keys, or local file paths committed
- [ ] If the SQLite schema changed: migration added in `crates/nexus-store/`
- [ ] If a new endpoint was added: auth, input validation, and SSE terminal events considered

## Screenshots / SSE traces (if UI or runtime behavior changed)

<!-- drag images here, or paste SSE event sequences -->

## Notes for reviewers

<!-- Anything reviewers should pay extra attention to, known follow-ups, etc. -->
