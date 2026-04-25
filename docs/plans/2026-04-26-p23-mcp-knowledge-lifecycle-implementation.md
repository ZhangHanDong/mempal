# P23 MCP Knowledge Lifecycle Implementation Plan

## Goal

Expose the Stage-1 knowledge lifecycle to MCP agents without weakening the governance model:

- `mempal_knowledge_promote` mutates status only after a deterministic gate pass.
- `mempal_knowledge_demote` requires counterexample evidence and a bounded reason type.
- CLI and MCP share one lifecycle implementation.

## Implementation Steps

1. Add a shared `knowledge_lifecycle` module for promote/demote validation, ref de-duplication, lifecycle updates, and audit writing.
2. Extend `knowledge_gate` with a reusable evaluator for an effective in-memory drawer so promotion can gate after appending request verification refs.
3. Refactor CLI lifecycle commands to call the shared implementation.
4. Add MCP request/response DTOs and tools for promote/demote.
5. Update protocol/docs/specs to describe gate-enforced MCP promotion.
6. Verify with focused MCP lifecycle tests, spec lint, cargo check, clippy, and full tests.

## Non-Goals

- No schema migration.
- No automatic distill-to-promote.
- No LLM evaluator or research integration.
- No REST lifecycle endpoint.
- No Phase-2 lifecycle event tables.
