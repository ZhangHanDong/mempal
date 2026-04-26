# P27 Knowledge Policy Surface Implementation Plan

**Goal:** Expose the current deterministic Stage-1 knowledge promotion policy without requiring a concrete drawer.

**Architecture:** Keep gate requirements in `src/knowledge_gate.rs` as the source of truth. Add a shared policy list, then wire it into CLI and MCP read-only surfaces.

## Steps

- [x] Validate task contract with `agent-spec parse/lint`.
- [x] Add shared promotion policy entry types and `promotion_policy()` helper.
- [x] Add `mempal knowledge policy --format plain|json`.
- [x] Add `mempal_knowledge_policy` MCP tool and DTO.
- [x] Add CLI and MCP tests for thresholds, reviewer rule, invalid format, and no side effects.
- [x] Update usage docs, MIND-MODEL-DESIGN, AGENTS, and CLAUDE.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p27-knowledge-policy-surface.spec.md
agent-spec lint specs/p27-knowledge-policy-surface.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
