# P16 Context Skill Guidance Implementation Plan

**Goal:** Teach MCP-connected agents how to consume `mempal_context` for workflow / skill / tool choice without turning memory into an executor.

**Architecture:** This is a protocol-only phase. Keep `mempal_context` response shape and runtime behavior unchanged; update the self-describing `MEMORY_PROTOCOL` plus docs and unit tests.

**Tech Stack:** Rust 2024, existing protocol string tests, agent-spec task contract.

## Source Spec

- `specs/p16-context-skill-guidance.spec.md`

## Tasks

- [x] Add protocol language for context-guided skill selection.
- [x] Add protocol tests for ordering, non-execution, precedence, and context/search split.
- [x] Update AGENTS.md, CLAUDE.md, usage docs, and mind-model design notes.
- [x] Run final verification and commit.

## Verification

```bash
agent-spec parse specs/p16-context-skill-guidance.spec.md
agent-spec lint specs/p16-context-skill-guidance.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo check --features rest
```
