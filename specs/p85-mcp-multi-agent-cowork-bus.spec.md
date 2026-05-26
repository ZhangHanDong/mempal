spec: task
name: "P85: MCP multi-agent cowork bus"
inherits: project
tags: [cowork, multi-agent, mcp, inbox, phase-4]
---

## Intent

P85 exposes the P84 multi-agent cowork bus to agent runtimes through MCP. Agents
should be able to register concrete `agent_id` instances, list the project bus,
send or broadcast messages, and drain their own per-agent inbox without falling
back to shell-only CLI coordination.

## Decisions

- Add one MCP tool `mempal_cowork_bus` with action-based requests.
- Supported actions are `register`, `list`, `send`, `broadcast`, and `drain`.
- The MCP tool reuses P84 file-backed registry and per-agent inbox functions.
- The MCP tool requires explicit `agent_id` values; it must not infer concrete
  instances from `client_info.name`.
- `send` and `broadcast` write only to per-agent bus inboxes under
  `~/.mempal/cowork-bus/<encoded_project_identity>/`.
- `drain` returns and consumes only the requested `agent_id` inbox.
- The tool must be read/write only for ephemeral bus files and must not write
  palace.db, drawers, cards, runtime adoption events, or schema state.
- tmux transport remains out of scope for P85.

## Boundaries

### Allowed Changes
- specs/p85-mcp-multi-agent-cowork-bus.spec.md
- docs/plans/2026-05-25-p85-mcp-multi-agent-cowork-bus.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/core/protocol.rs
- src/mcp/tools.rs
- src/mcp/server.rs

### Forbidden
- Do not change P84 CLI behavior.
- Do not change palace.db schema or `CURRENT_SCHEMA_VERSION`.
- Do not write drawers, triples, cards, runtime adoption events, or audit rows.
- Do not add a separate MCP tool for each bus action.
- Do not implement tmux send/capture in P85.
- Do not make legacy `mempal_cowork_push` depend on P84 registry.

## Acceptance Criteria

Scenario: MCP registers and lists concrete agents
  Test:
    Filter: test_mcp_cowork_bus_register_and_list
    Targets: `mempal_cowork_bus` register/list actions.
  Given a temporary HOME and one project cwd
  When calling `mempal_cowork_bus` with action `register` for `claude-main`, `codex-a`, and `codex-b`
  And calling action `list`
  Then the response contains all three registered agent ids
  And no palace.db side effect is required for bus state

Scenario: MCP send drains only the addressed agent
  Test:
    Filter: test_mcp_cowork_bus_send_drains_only_target
    Targets: `mempal_cowork_bus` send/drain actions.
  Given registered agents `claude-main`, `codex-a`, and `codex-b`
  When action `send` sends a message from `claude-main` to `codex-a`
  Then action `drain` for `codex-a` returns the message
  And action `drain` for `codex-b` returns zero messages

Scenario: MCP broadcast fans out per target
  Test:
    Filter: test_mcp_cowork_bus_broadcast_fans_out
    Targets: `mempal_cowork_bus` broadcast action.
  Given registered agents `claude-main`, `codex-a`, and `codex-b`
  When action `broadcast` sends one message to `codex-a` and `codex-b`
  Then draining each target returns one independent copy

Scenario: MCP rejects invalid action and addressing without mutation
  Test:
    Filter: test_mcp_cowork_bus_rejects_invalid_action_and_addressing
    Targets: MCP error mapping.
  Given a registered `codex-a`
  When action `register` uses agent id `bad/id`
  Then the call fails with invalid params
  When action `send` sends from `codex-a` to `codex-a`
  Then the call fails with invalid params
  When action `unknown` is used
  Then the call fails with invalid params

Scenario: Protocol advertises the multi-agent bus boundary
  Test:
    Filter: test_mcp_tool_registry_and_protocol_include_cowork_bus
    Targets: MCP tool registry and MEMORY_PROTOCOL.
  Given the MCP server tool registry and embedded protocol
  When inspecting tool names and instructions
  Then `mempal_cowork_bus` is present
  And protocol text says concrete `agent_id` bus routing is separate from legacy partner push

## Out of Scope

- tmux transport.
- Hook installation per `agent_id`.
- Auto-discovering agent ids from MCP client names.
- Durable bus message history.
- Replacing legacy `mempal_cowork_push`.
