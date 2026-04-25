# P24 Anchor Publication Implementation Plan

## Goal

Add explicit Stage-1 knowledge anchor publication so stable knowledge can move outward across persistence scopes without changing its tier/status lifecycle.

## Implementation Steps

1. Add `Database::update_knowledge_anchor` for metadata-only anchor updates.
2. Add `knowledge_anchor` shared logic for validation, target anchor resolution, update, and audit writing.
3. Add CLI command `mempal knowledge publish-anchor`.
4. Add integration tests for worktree-to-repo, repo-to-global, invalid chains, invalid targets, and schema stability.
5. Update docs, repo instructions, and mind-model implementation surface.

## Non-Goals

- No schema migration.
- No drawer clone/copy.
- No vector or FTS content rewrite.
- No MCP/REST endpoint.
- No automatic publication from promote, gate, or distill.
