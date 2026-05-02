spec: task
name: "P58: evaluator API evidence gate"
inherits: project
tags: [phase-3, evaluator, gate]
---

## Intent

P58 implements the first evaluator-facing Phase-3 surface as a read-only
evidence gate. Evaluator APIs remain advisory-only and cannot mutate lifecycle
state.

## Decisions

- Add `mempal phase3 gate evaluator-api`.
- The gate reads `evaluator` adoption events.
- The gate requires accepted evaluator advice and no rollback or contradiction signals.
- P50 advisory-only lifecycle boundaries remain binding.

## Boundaries

### Allowed Changes
- src/main.rs
- tests/phase3_runtime.rs
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add evaluator lifecycle write APIs.
- Do not satisfy reviewer requirements through evaluator output.
- Do not bypass deterministic gates.

## Acceptance Criteria

Scenario: evaluator gate is documented as advisory-only
  Test:
    Filter: rg -n "Evaluator API gate remains advisory-only|P50 advisory-only lifecycle boundary" docs/MIND-MODEL-DESIGN.md
  Given P58 documentation
  When reading evaluator gate policy
  Then evaluator APIs remain advisory-only

Scenario: evaluator gate exists
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_evaluator_gate_exists_and_is_read_only
  Given the CLI
  When evaluating the evaluator API candidate
  Then the command returns a read-only readiness report

## Out of Scope

- Evaluator scoring.
- Evaluator-driven promotion.
