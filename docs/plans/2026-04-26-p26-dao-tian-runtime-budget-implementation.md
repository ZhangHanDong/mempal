# P26 Dao Tian Runtime Budget Implementation Plan

**Goal:** Add a conservative runtime budget for `dao_tian` context injection so universal principles remain sparse by default while still allowing explicit caller override.

**Architecture:** Keep `src/context.rs` as the single source of assembly behavior. Add `dao_tian_limit` to the core request, expose it through CLI and MCP, and document the default policy in protocol and design docs. No schema, ranking, lifecycle, or wake-up changes.

## Steps

- [x] Validate task contract with `agent-spec parse/lint`.
- [x] Add `dao_tian_limit` to `ContextRequest` and enforce it only for the `dao_tian` tier.
- [x] Add CLI flag `--dao-tian-limit`, defaulting to 1.
- [x] Add MCP request field `dao_tian_limit`, defaulting to 1.
- [x] Add core, CLI, and MCP tests for default cap, disable, raised limit, and global `max_items` cap.
- [x] Update protocol, usage docs, mind-model design status, and repo instructions.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p26-dao-tian-runtime-budget.spec.md
agent-spec lint specs/p26-dao-tian-runtime-budget.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
