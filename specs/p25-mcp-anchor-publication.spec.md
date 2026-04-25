spec: task
name: "P25: MCP knowledge anchor publication"
inherits: project
tags: [memory, knowledge, anchor, lifecycle, mcp]
---

## Intent

P24 added CLI-only explicit anchor publication for active knowledge drawers, but MCP-connected agents still cannot publish stable worktree knowledge outward without shelling out. P25 exposes the same metadata-only operation as an MCP tool while reusing the P24 implementation and preserving the Stage-1 no-schema-change model.

## Decisions

- Add one MCP tool named `mempal_knowledge_publish_anchor`.
- The MCP tool reuses `publish_anchor`; CLI and MCP publication rules must not drift.
- Request fields are `drawer_id`, `to`, `reason`, optional `reviewer`, optional `target_anchor_id`, and optional `cwd`.
- Response fields are `drawer_id`, `old_anchor_kind`, `old_anchor_id`, `old_parent_anchor_id`, `new_anchor_kind`, `new_anchor_id`, and `new_parent_anchor_id`.
- The tool only accepts active knowledge drawers with status `promoted|canonical`.
- The tool preserves P24 outward-only publication chain:
  - `worktree -> repo`
  - `repo -> global`
  - reject `worktree -> global`
  - reject same-anchor and inward publication
- The tool is metadata-only:
  - updates `anchor_kind`, `anchor_id`, and `parent_anchor_id`
  - does not mutate content, statement, tier, status, refs, vectors, FTS content, source_file, triples, tunnels, schema, or Phase-2 objects
- Successful publication appends the same `knowledge_publish_anchor` audit entry as CLI.
- Invalid target drawer, inactive status, invalid chain, invalid target anchor, and invalid global-domain attempts return MCP invalid params errors.
- Database/audit write failures return MCP internal errors.
- `MEMORY_PROTOCOL`, docs, and repo instructions list the new MCP tool and state that anchor publication is separate from tier/status promotion.

## Boundaries

### Allowed Changes

- `src/knowledge_anchor.rs`
- `src/mcp/tools.rs`
- `src/mcp/server.rs`
- `src/core/protocol.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p25-mcp-anchor-publication.spec.md`
- `docs/plans/**`

### Forbidden

- Do not add tables, columns, triggers, or migrations.
- Do not add REST publication endpoints.
- Do not change CLI `publish-anchor` behavior.
- Do not duplicate or clone drawers.
- Do not update vectors or re-embed content.
- Do not auto-publish from promote, distill, or gate.
- Do not introduce new dependencies.

## Acceptance Criteria

Scenario: MCP publishes worktree knowledge to repo anchor
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_publish_anchor_worktree_to_repo
  Given a promoted knowledge drawer has `anchor_kind="worktree"` and `parent_anchor_id="repo://parent"`
  When an MCP client calls `mempal_knowledge_publish_anchor` with `to="repo"` and a reason
  Then the response reports old anchor `worktree` and new anchor `repo://parent`
  And the stored drawer has `anchor_kind="repo"`, `anchor_id="repo://parent"`, and `parent_anchor_id=NULL`
  And content, statement, status, refs, schema, and vector row remain unchanged
  And one `knowledge_publish_anchor` audit entry is appended

Scenario: MCP publishes repo knowledge to explicit global anchor
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_publish_anchor_repo_to_global
  Given a canonical knowledge drawer has `domain="global"` and `anchor_kind="repo"`
  When an MCP client calls `mempal_knowledge_publish_anchor` with `to="global"` and `target_anchor_id="global://epistemics"`
  Then the response reports `new_anchor_kind="global"` and `new_anchor_id="global://epistemics"`
  And the audit entry includes the supplied reviewer

Scenario: MCP rejects invalid publication without mutation
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_publish_anchor_rejects_invalid_chain_without_mutation
  Given a promoted knowledge drawer has `anchor_kind="worktree"`
  When an MCP client calls `mempal_knowledge_publish_anchor` with `to="global"` and `target_anchor_id="global://x"`
  Then the call fails with invalid params
  And the error mentions `worktree -> global publication is not allowed`
  And drawer anchor, schema, vector row, and audit log remain unchanged

Scenario: MCP rejects inactive or evidence targets
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_publish_anchor_rejects_inactive_or_evidence
  Given one evidence drawer id and one candidate knowledge drawer id
  When an MCP client calls `mempal_knowledge_publish_anchor` for the evidence drawer
  Then the call fails with invalid params and mentions `knowledge drawer`
  When an MCP client calls `mempal_knowledge_publish_anchor` for the candidate drawer
  Then the call fails with invalid params and mentions `promoted or canonical`

Scenario: MCP tool registry and protocol include anchor publication
  Test:
    Package: mempal
    Filter: test_mcp_tool_registry_and_protocol_include_knowledge_publish_anchor
  Given the MCP tool registry and `MEMORY_PROTOCOL`
  When they are inspected
  Then the registry contains `mempal_knowledge_publish_anchor`
  And the description mentions outward anchor publication
  And `MEMORY_PROTOCOL` says anchor publication is separate from tier/status promotion

## Out of Scope

- REST publication endpoint.
- Automatic publication.
- LLM or evaluator scoring.
- Phase-2 knowledge cards.
