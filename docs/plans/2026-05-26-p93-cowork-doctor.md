# P93 Cowork Doctor

Spec: `specs/p93-cowork-doctor.spec.md`

## Plan

- [x] Define P93 task contract and implementation plan.
- [x] Add read-only doctor report types and derivation in cowork bus core.
- [x] Add CLI `cowork-doctor`.
- [x] Add MCP `mempal_cowork_bus action=doctor`.
- [x] Add CLI/MCP tests for empty registry, stale/pending state, tmux probe, and read-only behavior.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p93-cowork-doctor.spec.md
agent-spec lint specs/p93-cowork-doctor.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_doctor_empty_registry
cargo test --test cowork_bus test_cli_cowork_doctor_reports_stale_and_pending
cargo test --test cowork_bus test_cli_cowork_doctor_json_tmux_probe
cargo test --lib test_mcp_cowork_bus_doctor
```
