# P56 Card Context Default Gate

## Goal

Add a read-only `mempal phase3 gate card-context-default` readiness check based
on runtime adoption evidence.

## Verification

```bash
agent-spec parse specs/p56-card-context-default-gate.spec.md
agent-spec lint specs/p56-card-context-default-gate.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_record_stats_and_gate
```
