spec: task
name: "P40: MCP knowledge card lifecycle actions"
inherits: project
tags: [memory, mind-model, knowledge-cards, mcp, phase-2]
---

## Intent

P40 exposes Phase-2 knowledge card gate and lifecycle operations through the
existing `mempal_knowledge_cards` MCP tool so agents can inspect and govern
cards without shelling out.

## Decisions

- Extend `mempal_knowledge_cards` actions to `list/get/events/gate/promote/demote`.
- Keep card creation out of MCP.
- `gate` is read-only.
- `promote` and `demote` reuse the same core logic as the CLI.
- Tool description and embedded protocol must mention the new action set.

## Acceptance Criteria

Scenario: MCP card lifecycle actions mutate through shared core
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_gate_and_lifecycle_actions
  Given one card and evidence drawers
  When MCP gate, promote, and demote actions are called
  Then promote and demote update card lifecycle and append events

Scenario: unknown MCP card action is rejected
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_rejects_unknown_actions_without_mutation
  Given an initialized server
  When calling an unsupported card action
  Then the request fails
  And card rows are unchanged

Scenario: MCP protocol advertises card lifecycle
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_knowledge_cards_lifecycle
  Given MCP tool registry
  When inspecting `mempal_knowledge_cards`
  Then description and protocol mention list/get/events/gate/promote/demote

## Out of Scope

- Do not add a separate MCP tool for cards.
- Do not expose card creation via MCP.
- Do not change REST.

## Constraints

- MCP and CLI lifecycle paths must share core functions.
- MCP error mapping must distinguish invalid params from internal write failures.
