# P34 Knowledge Card CLI Implementation Plan

**Goal:** Add the first operator-facing CLI surface for Phase-2 knowledge cards
without exposing MCP, REST, search, context, wake-up, lifecycle automation, or
backfill behavior.

**Architecture:** Add a top-level `knowledge-card` command family in
`src/main.rs` that calls the P33 `Database` APIs. Keep JSON/plain output helpers
local to the CLI and keep card evidence as drawer links.

## Steps

- [x] Add P34 task contract.
- [x] Add `knowledge-card` CLI subcommands for create/get/list/link/event/events.
- [x] Add parsing and output helpers for card enums, filters, links, and events.
- [x] Add CLI integration tests covering create/get, list filters, link validation,
      event history, and invalid format errors.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p34-knowledge-card-cli.spec.md
agent-spec lint specs/p34-knowledge-card-cli.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
