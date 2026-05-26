# P100 Guided Maintenance Run

Spec: `specs/p100-guided-maintenance-run.spec.md`

## Tasks

- [x] Define the P100 task contract and implementation plan.
- [x] Add `maintenance guided-run --format plain|json`.
- [x] Build deterministic state counters and command suggestions.
- [x] Add integration tests for JSON, plain, and invalid format.
- [x] Update AGENTS / CLAUDE / MIND-MODEL inventory.

## Verification

```bash
agent-spec parse specs/p100-guided-maintenance-run.spec.md
agent-spec lint specs/p100-guided-maintenance-run.spec.md --min-score 0.7
cargo test --test ops_runtime test_cli_maintenance_guided_run_json
cargo test --test ops_runtime test_cli_maintenance_guided_run_plain
cargo test --test ops_runtime test_cli_maintenance_guided_run_rejects_invalid_format
```
