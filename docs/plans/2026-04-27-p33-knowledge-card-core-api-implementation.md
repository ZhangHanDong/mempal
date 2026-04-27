# P33 Knowledge Card Core API Implementation Plan

**Goal:** Add typed Rust structs and core `Database` APIs for schema v8
knowledge cards, evidence links, and events without exposing a runtime surface.

**Architecture:** Keep the implementation in `src/core/types.rs` and
`src/core/db.rs`. Use existing enum vocabulary and SQLite constraints. Validate
evidence links at the API layer so only active evidence drawers can be linked.

## Steps

- [x] Add P33 task contract.
- [x] Add knowledge card / link / event Rust types.
- [x] Add DB insert/get/list/update APIs for cards.
- [x] Add DB link/list APIs for evidence links.
- [x] Add DB append/list APIs for events.
- [x] Add integration tests over the typed DB APIs.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p33-knowledge-card-core-api.spec.md
agent-spec lint specs/p33-knowledge-card-core-api.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
