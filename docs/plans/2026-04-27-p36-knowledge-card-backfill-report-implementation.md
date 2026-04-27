# P36 Knowledge Card Backfill Report Implementation Plan

**Goal:** Add a read-only dry-run report that identifies Stage-1
`memory_kind=knowledge` drawers that can later become Phase-2 knowledge cards.

**Architecture:** Add a small core module that reads Stage-1 knowledge drawers
through `Database`, maps them to deterministic prospective card ids, and returns
ready/skipped/already-existing candidates. Expose it through
`mempal knowledge-card backfill-plan` only.

## Steps

- [x] Add P36 task contract.
- [x] Add core report types and deterministic prospective id mapping.
- [x] Add `Database` read helpers for knowledge drawer candidates and card count.
- [x] Add CLI `knowledge-card backfill-plan` with plain and JSON output.
- [x] Add core and CLI tests for classification, read-only behavior, filters, and invalid format.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p36-knowledge-card-backfill-report.spec.md
agent-spec lint specs/p36-knowledge-card-backfill-report.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
