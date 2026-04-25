# P25 MCP Anchor Publication Implementation Plan

## Goal

Expose P24 explicit knowledge anchor publication to MCP-connected agents without changing the underlying Stage-1 drawer model.

## Implementation Steps

1. Add MCP DTOs for `mempal_knowledge_publish_anchor`.
2. Add MCP server handler that reuses `publish_anchor`.
3. Map publication validation errors to invalid params and write failures to internal errors.
4. Add MCP integration tests for worktree-to-repo, repo-to-global, invalid chain, invalid targets, and tool registry/protocol coverage.
5. Update protocol, docs, repo instructions, and mind-model implemented surface.

## Non-Goals

- No REST endpoint.
- No schema migration.
- No drawer clone/copy.
- No vector rewrite or re-embedding.
- No automatic publication from lifecycle actions.
