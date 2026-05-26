# P95 Cowork Handoff Summary

Spec: `specs/p95-cowork-handoff-summary.spec.md`

## Plan

- [x] Define P95 task contract and implementation plan.
- [x] Add deterministic handoff summary builder in cowork bus core.
- [x] Add CLI `cowork-handoff`.
- [x] Add MCP `mempal_cowork_bus action=handoff`.
- [x] Add CLI/MCP tests for summary shape, filters, invalid format, and read-only behavior.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p95-cowork-handoff-summary.spec.md
agent-spec lint specs/p95-cowork-handoff-summary.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_handoff_plain
cargo test --test cowork_bus test_cli_cowork_handoff_filters_thread
cargo test --test cowork_bus test_cli_cowork_handoff_rejects_invalid_format
cargo test --lib test_mcp_cowork_bus_handoff
```
