# P44 Card-Aware Context Assembler Plan

**Goal:** Add explicit opt-in card context items while preserving default
drawer-only runtime context.

## Steps

- [x] Add P44 task contract.
- [x] Extend context request/items with optional card metadata and citations.
- [x] Add card assembly under existing tier sections behind `include_cards`.
- [x] Wire CLI `--include-cards`.
- [x] Wire MCP `include_cards`.
- [x] Add core, CLI, and MCP tests.
- [x] Update MIND-MODEL / AGENTS / CLAUDE inventories.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p44-card-context-assembler.spec.md
agent-spec lint specs/p44-card-context-assembler.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
