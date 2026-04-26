# P29 Wake-Up / Context Boundary Implementation Plan

**Goal:** Resolve the remaining design question by keeping `wake-up` as an
L0/L1 memory refresh and keeping typed `dao_tian -> dao_ren -> shu -> qi`
assembly exclusive to `mempal context` / `mempal_context`.

**Architecture:** This is a boundary-hardening task. Update protocol and docs,
then add regression tests that prevent wake-up from growing typed mind-model
sections or tier budgets.

## Steps

- [x] Validate task contract with `agent-spec parse/lint`.
- [x] Update MEMORY_PROTOCOL with explicit wake-up/context responsibility split.
- [x] Add wake-up regression tests for plain and AAAK output.
- [x] Update usage docs and MIND-MODEL-DESIGN.
- [x] Update AGENTS and CLAUDE spec inventories.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p29-wake-up-context-boundary.spec.md
agent-spec lint specs/p29-wake-up-context-boundary.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
