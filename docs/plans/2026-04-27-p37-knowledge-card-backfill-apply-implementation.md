# P37 Knowledge Card Backfill Apply Implementation Plan

**Goal:** Materialize P36 ready candidates into Phase-2 knowledge cards through
an explicit, safe-by-default command.

**Architecture:** Extend `knowledge_card_backfill` with an apply function that
reuses the P36 report. Dry-run performs no writes. Execute creates cards,
best-effort evidence links, and one created event per card. Stage-1 drawers stay
untouched.

## Steps

- [x] Add P37 task contract.
- [x] Add backfill apply result types and execute/dry-run core API.
- [x] Create cards from Stage-1 drawer metadata.
- [x] Link evidence refs by role with observable link errors.
- [x] Append created events for created cards.
- [x] Add CLI `knowledge-card backfill-apply`.
- [x] Add core and CLI tests for dry-run, execute, idempotency, invalid refs, filters, and invalid format.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p37-knowledge-card-backfill-apply.spec.md
agent-spec lint specs/p37-knowledge-card-backfill-apply.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
