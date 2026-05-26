spec: task
name: "P102: MCP cognitive brief"
inherits: project
tags: [brief, mcp, cognition, phase-5]
---

## Intent

P102 exposes the deterministic P83 cognitive brief to agent runtimes. Agents
should be able to request a citation-first brief before starting work without
falling back to raw search or ad hoc context assembly.

## Decisions

- Add MCP tool `mempal_brief`.
- The tool accepts query, field, domain, cwd, max_items, and dao_tian_limit.
- The tool uses the existing deterministic brief assembly path.
- The response includes summary, key facts, evidence, cards, entities,
  unresolved items, uncertainty, and next actions.
- The tool is read-only and does not write runtime adoption evidence.

## Boundaries

### Allowed Changes
- specs/p102-mcp-cognitive-brief.spec.md
- docs/plans/2026-05-26-p102-mcp-cognitive-brief.md
- AGENTS.md
- CLAUDE.md
- docs/MIND-MODEL-DESIGN.md
- src/mcp/tools.rs
- src/mcp/server.rs
- src/core/protocol.rs

### Forbidden
- Do not call an LLM.
- Do not write DB rows, adoption events, cards, or audit logs.
- Do not replace `mempal_search` or `mempal_context`.

## Acceptance Criteria

Scenario: MCP brief returns citation-first sections
  Test:
    Filter: test_mcp_brief_returns_cognitive_brief
    Targets: `mempal_brief`.
  Given a test DB with cited evidence and knowledge
  When `mempal_brief` runs
  Then the response includes a summary
  And it includes at least one key fact or evidence item
  And returned items include drawer/source citations

Scenario: MCP brief rejects invalid max_items
  Test:
    Filter: test_mcp_brief_rejects_max_items_zero
    Targets: request validation.
  When `mempal_brief` runs with `max_items=0`
  Then it returns invalid params

Scenario: MCP registry and protocol mention mempal_brief
  Test:
    Filter: test_mcp_tool_registry_and_protocol_include_mempal_brief
    Targets: tool registry and protocol.
  When inspecting MCP tools and memory protocol
  Then `mempal_brief` is advertised

## Out of Scope

- LLM synthesis.
- Adoption capture.
- CLI brief changes.
