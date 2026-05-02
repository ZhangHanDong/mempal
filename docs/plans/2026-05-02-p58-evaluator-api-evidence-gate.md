# P58 Evaluator API Evidence Gate

## Goal

Add a read-only `mempal phase3 gate evaluator-api` readiness check that
preserves evaluator advisory-only boundaries.

## Verification

```bash
agent-spec parse specs/p58-evaluator-api-evidence-gate.spec.md
agent-spec lint specs/p58-evaluator-api-evidence-gate.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_evaluator_gate_exists_and_is_read_only
```
