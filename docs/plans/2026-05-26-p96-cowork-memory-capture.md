# P96 Cowork Memory Capture

Spec: `specs/p96-cowork-memory-capture.spec.md`

## Plan

- [x] Define P96 task contract and implementation plan.
- [x] Add explicit capture plan and execute writer in cowork bus core.
- [x] Add CLI `cowork-capture`.
- [x] Add MCP `mempal_cowork_bus action=capture`.
- [x] Add CLI/MCP tests for dry-run, execute, unsupported source, and no automatic side effects.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p96-cowork-memory-capture.spec.md
agent-spec lint specs/p96-cowork-memory-capture.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_capture_dry_run_no_write
cargo test --test cowork_bus test_cli_cowork_capture_execute_writes_evidence
cargo test --test cowork_bus test_cli_cowork_capture_rejects_unknown_source
cargo test --lib test_mcp_cowork_bus_capture
```
