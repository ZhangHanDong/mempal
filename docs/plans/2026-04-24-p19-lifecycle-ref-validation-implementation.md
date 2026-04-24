# P19 Lifecycle Ref Validation Implementation Plan

**Goal:** Harden P17 lifecycle commands so promotion and demotion refs must point to evidence drawers, matching P18 distill ref discipline.

**Architecture:** Keep validation in the CLI lifecycle path for Stage 1. Reuse existing `Database::get_drawer` and `MemoryKind` metadata; do not add schema, MCP/REST endpoints, or evaluator logic.

## Task 1: Contract And Tests

- [x] Add `specs/p19-lifecycle-ref-validation.spec.md`.
- [x] Add failing integration tests for malformed refs, missing refs, wrong memory kind, and accepted evidence refs.
- [x] Run focused test selectors and confirm failures before implementation.

## Task 2: Lifecycle Ref Validation

- [x] Replace existence-only lifecycle ref validation with evidence drawer validation.
- [x] Preserve existing stable de-duplication and audit behavior.
- [x] Keep success output and lifecycle state mutation unchanged.

## Task 3: Docs And Verification

- [x] Update usage and project status docs for P19.
- [x] Update `MIND-MODEL-DESIGN.md` implemented Stage-1 surface.
- [x] Run spec parse/lint.
- [x] Run Rust formatting, clippy, targeted lifecycle tests, full tests, and `--features rest` check.
- [ ] Commit, ingest project memory, push branch, and open PR.

## Verification Commands

```bash
agent-spec parse specs/p19-lifecycle-ref-validation.spec.md
agent-spec lint specs/p19-lifecycle-ref-validation.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --test knowledge_lifecycle -- --nocapture
cargo test
cargo check --features rest
```
