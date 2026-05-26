# P102 MCP Cognitive Brief

Spec: `specs/p102-mcp-cognitive-brief.spec.md`

## Tasks

- [x] Define the P102 task contract and implementation plan.
- [x] Add MCP request/response DTOs for cognitive briefs.
- [x] Add `mempal_brief` MCP tool using existing brief core.
- [x] Add MCP behavior, validation, and registry/protocol tests.
- [x] Update AGENTS / CLAUDE / MIND-MODEL inventory.

## Verification

```bash
agent-spec parse specs/p102-mcp-cognitive-brief.spec.md
agent-spec lint specs/p102-mcp-cognitive-brief.spec.md --min-score 0.7
cargo test --lib test_mcp_brief_returns_cognitive_brief
cargo test --lib test_mcp_brief_rejects_max_items_zero
cargo test --lib test_mcp_tool_registry_and_protocol_include_mempal_brief
```
