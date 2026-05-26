spec: task
name: "P99: MCP runtime doctor"
inherits: project
tags: [doctor, mcp, runtime, phase-5]
---

## Intent

P99 exposes the P98 diagnostic surface to agent runtimes. Agents need a direct
MCP call that confirms the server they are connected to knows the current
tooling surface, especially after installing a newer binary where old MCP
processes may still be running.

## Decisions

- Add MCP tool `mempal_doctor`.
- `mempal_doctor` returns P98 release/install diagnostics plus MCP tool/action
  expectations for the current server.
- The MCP response includes whether required tools are advertised:
  `mempal_context`, `mempal_brief`, `mempal_phase3`, and `mempal_cowork_bus`.
- The response includes expected `mempal_cowork_bus` and `mempal_phase3`
  actions as static capability metadata.
- The tool is read-only and does not open the DB through migration paths.

## Boundaries

### Allowed Changes
- specs/p99-mcp-runtime-doctor.spec.md
- docs/plans/2026-05-26-p99-mcp-runtime-doctor.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/doctor.rs
- src/mcp/tools.rs
- src/mcp/server.rs
- src/core/protocol.rs

### Forbidden
- Do not restart MCP clients or server processes.
- Do not install hooks or modify user config.
- Do not infer client-side tool visibility beyond the server's advertised tools.

## Acceptance Criteria

Scenario: MCP doctor is registered and documented
  Test:
    Filter: test_mcp_tool_registry_includes_mempal_doctor
    Targets: `src/mcp/server.rs` tool registry.
  When listing MCP tools
  Then `mempal_doctor` is present
  And its description mentions MCP runtime diagnostics

Scenario: MCP doctor reports required server tools
  Test:
    Filter: test_mcp_doctor_reports_runtime_tools
    Targets: `mempal_doctor`.
  Given a test MCP server
  When `mempal_doctor` runs
  Then the response reports `mempal_context`
  And the response reports `mempal_phase3`
  And the response reports `mempal_cowork_bus`

Scenario: Protocol advertises mempal_doctor
  Test:
    Filter: test_mcp_tool_registry_and_protocol_include_mempal_doctor
    Targets: `src/core/protocol.rs`.
  When inspecting the memory protocol
  Then it mentions `mempal_doctor`

## Out of Scope

- Client process restart automation.
- Remote MCP client introspection.
- GitHub or network diagnostics.
