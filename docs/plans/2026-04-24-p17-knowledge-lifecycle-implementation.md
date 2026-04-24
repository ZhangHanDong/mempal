# P17 Knowledge Lifecycle Implementation Plan

**Goal:** Add the smallest bootstrap lifecycle surface for knowledge drawers: manual promote / demote through CLI, using existing metadata fields and audit log.

**Architecture:** Keep lifecycle as a CLI-only metadata update in P17. Add a narrow DB helper that updates `status`, `verification_refs`, and `counterexample_refs` without touching content, FTS, vectors, schema, or context assembly rules.

**Tech Stack:** Rust 2024, clap subcommands, rusqlite metadata update, existing `audit.jsonl`, integration tests under `tests/`.

## Source Spec

- `specs/p17-knowledge-lifecycle.spec.md`

## Tasks

- [x] Add `mempal knowledge promote|demote` CLI commands.
- [x] Add DB helper for metadata-only lifecycle updates.
- [x] Enforce knowledge-only, tier/status, target-status, and required-ref gates.
- [x] Append lifecycle audit entries.
- [x] Add integration tests for promote, demote, invalid drawer kind, invalid tier/status, audit, and schema/vector stability.
- [x] Update AGENTS.md, CLAUDE.md, usage docs, and mind-model design notes.
- [x] Run final verification and commit.

## Verification

```bash
agent-spec parse specs/p17-knowledge-lifecycle.spec.md
agent-spec lint specs/p17-knowledge-lifecycle.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo check --features rest
```
