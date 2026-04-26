spec: task
name: "P27: read-only knowledge promotion policy surface"
inherits: project
tags: [memory, knowledge, lifecycle, policy, mcp, cli]
---

## Intent

P20/P21 implemented deterministic promotion gates, but the policy is only visible
when evaluating a concrete drawer. `docs/MIND-MODEL-DESIGN.md` still lists
promotion thresholds and `dao_tian` human review as open questions even though
the current Stage-1 runtime already has a concrete policy.

This task adds a read-only policy surface so humans and MCP-connected agents can
inspect the current Stage-1 promotion policy without needing a fixture drawer.

## Decisions

- Add CLI command `mempal knowledge policy`.
- CLI supports `--format plain|json`, defaulting to `plain`.
- Add MCP tool `mempal_knowledge_policy`.
- The policy response lists deterministic entries for:
  - `dao_tian -> canonical`
  - `dao_ren -> promoted`
  - `shu -> promoted`
  - `qi -> promoted`
- Each entry exposes the same requirement fields used by `mempal knowledge gate`:
  - `min_supporting_refs`
  - `min_verification_refs`
  - `min_teaching_refs`
  - `reviewer_required`
  - `counterexamples_block`
- The policy source of truth must be shared with gate evaluation so documentation,
  CLI, MCP, and enforcement do not drift.
- `dao_tian -> canonical` keeps `reviewer_required=true`; Stage 1 does not allow
  evaluator-only canonization.
- The command and MCP tool are read-only and must not open or mutate drawers,
  vectors, triples, schema, or audit logs.

## Acceptance Criteria

Scenario: CLI prints promotion policy as JSON
  Test:
    Package: mempal
    Filter: test_cli_knowledge_policy_json_lists_stage1_thresholds
  Given any initialized mempal home
  When running `mempal knowledge policy --format json`
  Then the command exits successfully
  And JSON includes `dao_tian -> canonical` with supporting 3, verification 2, teaching 1, and reviewer required
  And JSON includes `dao_ren -> promoted` with supporting 2 and verification 1

Scenario: CLI prints promotion policy as plain text
  Test:
    Package: mempal
    Filter: test_cli_knowledge_policy_plain_lists_reviewer_rule
  Given any initialized mempal home
  When running `mempal knowledge policy`
  Then stdout includes `dao_tian -> canonical`
  And stdout includes `reviewer_required=true`

Scenario: MCP exposes the same promotion policy
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_policy_lists_stage1_thresholds
  Given an MCP server
  When calling `mempal_knowledge_policy`
  Then the response includes the same `dao_tian -> canonical` and `dao_ren -> promoted` thresholds as CLI policy

Scenario: MCP policy has no database side effects
  Test:
    Package: mempal
    Filter: test_mcp_knowledge_policy_has_no_db_side_effects
  Given a database with known schema, drawer, triple, and taxonomy counts
  When calling `mempal_knowledge_policy` repeatedly
  Then those counts remain unchanged

Scenario: CLI rejects unsupported policy output format
  Test:
    Package: mempal
    Filter: test_cli_knowledge_policy_rejects_invalid_format
  Given any initialized mempal home
  When running `mempal knowledge policy --format yaml`
  Then the command fails with an unsupported format error

## Out of Scope

- Do not change existing gate thresholds.
- Do not add configurable promotion policy.
- Do not add evaluator-only `dao_tian` canonization.
- Do not change lifecycle mutation behavior.
- Do not change context or wake-up assembly.
- Do not change database schema.

## Constraints

- Keep `src/knowledge_gate.rs` as the source of truth for gate requirements.
- Keep CLI and MCP policy responses aligned.
- Update `docs/MIND-MODEL-DESIGN.md` so thresholds and `dao_tian` reviewer policy are no longer listed as open questions.
