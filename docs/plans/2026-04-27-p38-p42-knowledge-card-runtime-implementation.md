# P38-P42 Knowledge Card Runtime Implementation Plan

**Goal:** Close the current MIND-MODEL baseline by giving Phase-2 knowledge cards
readiness gates, governed lifecycle mutation, MCP access, and explicit runtime
boundary documentation.

## Steps

- [x] Add P38-P42 task contracts.
- [x] Add Phase-2 card gate core and CLI.
- [x] Add Phase-2 card promote/demote core and CLI.
- [x] Extend `mempal_knowledge_cards` with gate/promote/demote actions.
- [x] Update protocol and MIND-MODEL runtime boundary docs.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p38-knowledge-card-gate.spec.md
agent-spec parse specs/p39-knowledge-card-lifecycle-cli.spec.md
agent-spec parse specs/p40-mcp-knowledge-card-lifecycle.spec.md
agent-spec parse specs/p41-knowledge-card-runtime-boundary.spec.md
agent-spec parse specs/p42-mind-model-completion-audit.spec.md
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
