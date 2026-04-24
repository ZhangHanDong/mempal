# P18 Knowledge Distill Implementation Plan

**Goal:** Add the missing Phase-1 `distill` operation: create candidate knowledge drawers from existing evidence refs through CLI.

**Architecture:** Keep distill as a CLI-only bootstrap operation. Reuse existing drawer schema, bootstrap identity, knowledge source URI, embedder, and audit log. Do not add schema, MCP/REST endpoints, or automatic promotion/evaluation.

**Tech Stack:** Rust 2024, clap subcommands, existing `Database`, existing embedder factory, integration tests under `tests/`.

## Source Spec

- `specs/p18-knowledge-distill.spec.md`

## Tasks

- [x] Add `mempal knowledge distill` CLI arguments.
- [x] Build candidate knowledge drawer using bootstrap identity and existing knowledge source URI.
- [x] Validate tier/status constraints and role refs.
- [x] Support optional trigger hints and scope constraints.
- [x] Implement dry-run and idempotent existing-drawer behavior.
- [x] Append `knowledge_distill` audit entry on successful create.
- [x] Add integration tests for create, dry-run, invalid tier, missing refs, trigger hints, audit, and schema stability.
- [x] Update AGENTS.md, CLAUDE.md, usage docs, and mind-model design notes.
- [x] Run final verification and commit.

## Verification

```bash
agent-spec parse specs/p18-knowledge-distill.spec.md
agent-spec lint specs/p18-knowledge-distill.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo check --features rest
```
