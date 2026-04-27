# P35 Knowledge Card MCP Read Implementation Plan

**Goal:** Add a read-only MCP inspection tool for Phase-2 knowledge cards:
`mempal_knowledge_cards` with `list`, `get`, and `events` actions.

**Architecture:** Keep DTOs in `src/mcp/tools.rs` and dispatch in
`src/mcp/server.rs`, reusing P33 `Database` read APIs. Update protocol text and
repo inventories. Do not expose MCP writes.

## Steps

- [x] Add P35 task contract.
- [x] Add knowledge card MCP DTOs and response conversions.
- [x] Add `mempal_knowledge_cards` action dispatch for `list/get/events`.
- [x] Add protocol/tool registry read-only guidance.
- [x] Add MCP tests for filters, get/missing, events, rejected writes, and registry/protocol.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p35-knowledge-card-mcp-read.spec.md
agent-spec lint specs/p35-knowledge-card-mcp-read.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
