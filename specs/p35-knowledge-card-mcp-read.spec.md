spec: task
name: "P35: knowledge card MCP read API"
inherits: project
tags: [memory, mind-model, knowledge-cards, mcp, phase-2]
---

## Intent

P34 exposed Phase-2 knowledge cards through a local CLI. P35 adds the first MCP
read surface so agents can inspect cards and event history from the same schema
v8 tables without gaining write access through MCP.

## Decisions

- Add one MCP tool named `mempal_knowledge_cards`.
- Support read-only actions: `list`, `get`, and `events`.
- `list` accepts the same filters as `KnowledgeCardFilter`: tier, status,
  domain, field, anchor_kind, and anchor_id.
- `get` requires `card_id` and returns exactly one card or an MCP error when
  missing.
- `events` requires `card_id` and returns append-only events for that card.
- Reuse P33 `Database` read APIs; do not write direct SQL in the MCP handler.
- Include the tool in `MEMORY_PROTOCOL` as a Phase-2 read-only inspection tool.
- Do not add MCP write actions for create, update, delete, link, or event append.
- Do not add REST, CLI changes beyond docs inventory, search, context, wake-up,
  lifecycle automation, promotion gates, or backfill behavior.

## Acceptance Criteria

Scenario: MCP list returns filtered cards
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_list_filters
  Given schema v8 with multiple knowledge cards
  When calling `mempal_knowledge_cards` with action `list` and tier/status/field filters
  Then only matching cards are returned

Scenario: MCP get returns one card or missing error
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_get_and_missing
  Given schema v8 with one knowledge card
  When calling `mempal_knowledge_cards` with action `get` and its card id
  Then the returned card includes statement, content, tier, status, domain, field, and anchor metadata
  When calling action `get` for a missing card id
  Then the tool returns an MCP error

Scenario: MCP events returns event history
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_events
  Given schema v8 with one knowledge card and two events
  When calling `mempal_knowledge_cards` with action `events`
  Then the response contains those events ordered by created_at and id

Scenario: MCP rejects write actions
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_cards_rejects_write_actions
  Given schema v8
  When calling `mempal_knowledge_cards` with action `create`
  Then the tool returns an invalid params error
  And no knowledge card rows are written

Scenario: tool registry and protocol expose read-only boundary
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_knowledge_cards_readonly
  Given a mempal MCP server
  When listing registered MCP tools and reading `MEMORY_PROTOCOL`
  Then `mempal_knowledge_cards` is present
  And its description and protocol text say it is read-only

## Out of Scope

- Do not add MCP card create.
- Do not add MCP card update.
- Do not add MCP card delete.
- Do not add MCP evidence link writes.
- Do not add MCP event append writes.
- Do not add REST endpoints.
- Do not change CLI behavior.
- Do not change search results.
- Do not change context assembly.
- Do not change wake-up.
- Do not backfill existing `memory_kind=knowledge` drawers into cards.

## Constraints

- Keep the MCP surface action-based to match existing `mempal_tunnels` and
  `mempal_kg` style.
- Keep all returned records source-backed by schema v8 card/event ids.
- Invalid actions must fail before any database mutation.
