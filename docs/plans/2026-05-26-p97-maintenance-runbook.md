# P97 Maintenance Runbook

Spec: `specs/p97-maintenance-runbook.spec.md`

## Plan

- [x] Define P97 task contract and implementation plan.
- [x] Add `docs/MAINTENANCE-RUNBOOK.md`.
- [x] Add read-only CLI `maintenance-runbook --format plain|json`.
- [x] Add CLI tests for plain, JSON, invalid format, and side effects.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p97-maintenance-runbook.spec.md
agent-spec lint specs/p97-maintenance-runbook.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_maintenance_runbook_plain
cargo test --test cowork_bus test_cli_maintenance_runbook_json
cargo test --test cowork_bus test_cli_maintenance_runbook_rejects_invalid_format
rg -n "p97-maintenance-runbook|P97 maintenance runbook|MAINTENANCE-RUNBOOK" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
```
