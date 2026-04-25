# P20 Promotion Gate Policy Implementation Plan

**Goal:** Add a read-only `mempal knowledge gate` command that evaluates whether a Stage-1 knowledge drawer satisfies deterministic promotion policy before a human or agent runs `promote`.

**Architecture:** Keep P20 advisory and schema-free. Implement a small core policy helper that can be tested through the existing lifecycle integration harness, then wire it into the `knowledge` CLI group. Do not mutate drawers, vectors, audit logs, or context behavior.

## Task 1: Contract And Failing Tests

- [x] Add `specs/p20-promotion-gate-policy.spec.md`.
- [x] Add failing integration tests in `tests/knowledge_lifecycle.rs` for allow, reject, counterexample, reviewer, invalid target, and bad refs.
- [x] Run targeted test selectors to confirm the command is missing/failing.

## Task 2: Gate Policy Implementation

- [x] Add `mempal knowledge gate <drawer_id>` CLI args: `--target-status`, `--reviewer`, `--allow-counterexamples`, `--format plain|json`.
- [x] Add read-only gate report structs and deterministic evidence-count policy.
- [x] Reuse existing tier/status and evidence-ref validation rules.
- [x] Ensure gate does not write audit entries or mutate drawer/vector/schema state.

## Task 3: Docs And Verification

- [x] Update `docs/usage.md` with gate examples.
- [x] Update `docs/MIND-MODEL-DESIGN.md` implemented Stage-1 surface.
- [x] Update `AGENTS.md` and `CLAUDE.md` status tables.
- [x] Run spec parse/lint.
- [x] Run `cargo fmt -- --check`, `cargo check`, clippy, targeted lifecycle tests, full tests, and `cargo check --features rest`.
- [ ] Commit, ingest project memory, push branch, and open PR.

## Verification Commands

```bash
agent-spec parse specs/p20-promotion-gate-policy.spec.md
agent-spec lint specs/p20-promotion-gate-policy.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --test knowledge_lifecycle -- --nocapture
cargo test
cargo check --features rest
```
