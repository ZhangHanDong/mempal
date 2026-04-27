# P32 Knowledge Card Schema v8 Implementation Plan

**Goal:** Implement the Phase-2 schema contract from P31 as SQLite schema v8.

**Architecture:** Add a single additive migration in `src/core/db.rs` that
creates `knowledge_cards`, `knowledge_evidence_links`, and `knowledge_events`,
with constraints, indexes, foreign keys, and append-only event triggers. Add
integration tests that exercise the schema directly through raw SQL. Do not add
runtime APIs yet.

## Steps

- [x] Add P32 task contract.
- [x] Bump current schema version to 8.
- [x] Enable SQLite foreign-key enforcement on opened DB connections.
- [x] Add v8 migration SQL for the three knowledge card tables.
- [x] Add schema v8 integration tests.
- [x] Update AGENTS / CLAUDE inventories and current schema notes.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p32-knowledge-card-schema-v8.spec.md
agent-spec lint specs/p32-knowledge-card-schema-v8.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
