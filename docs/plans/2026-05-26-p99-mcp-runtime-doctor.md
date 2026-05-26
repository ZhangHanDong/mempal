# P99 MCP Runtime Doctor

Spec: `specs/p99-mcp-runtime-doctor.spec.md`

## Tasks

- [x] Define the P99 task contract and implementation plan.
- [x] Add MCP request/response DTOs for doctor diagnostics.
- [x] Add `mempal_doctor` server tool backed by P98 doctor core.
- [x] Add MCP registry/protocol tests.
- [x] Update AGENTS / CLAUDE / MIND-MODEL inventory.

## Verification

```bash
agent-spec parse specs/p99-mcp-runtime-doctor.spec.md
agent-spec lint specs/p99-mcp-runtime-doctor.spec.md --min-score 0.7
cargo test --lib test_mcp_tool_registry_includes_mempal_doctor
cargo test --lib test_mcp_doctor_reports_runtime_tools
cargo test --lib test_mcp_tool_registry_and_protocol_include_mempal_doctor
```
