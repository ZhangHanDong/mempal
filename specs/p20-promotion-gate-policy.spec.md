spec: task
name: "P20: promotion gate policy"
inherits: project
tags: [memory, knowledge, lifecycle, gate, cli]
estimate: 0.5d
---

## Intent

P12-P19 已经完成 Stage-1 mind-model bootstrap：agent 可以记录 evidence、distill candidate knowledge、手动 promote/demote，并通过 CLI/MCP context 唤醒 active knowledge。P20 增加一个只读 promotion gate report，让人工或 agent 在 promote 前明确看到某条 knowledge 是否满足最小治理门槛。

本任务只实现 policy evaluation 和 dry-run report，不自动 promote，不引入 evaluator scoring，不新增 schema，也不进入 Phase-2 `knowledge_cards`。

## Decisions

- 新增 CLI：`mempal knowledge gate <drawer_id>`
- Gate command is read-only:
  - does not update drawer status
  - does not mutate refs
  - does not write vectors
  - does not append audit entries
  - does not bump schema
- Gate command only accepts `memory_kind=knowledge`
- Gate command returns non-zero for missing drawers or evidence drawers
- Gate command supports report formats:
  - default plain text
  - `--format json`
- Gate report includes drawer id, tier, current status, target status, allowed flag, reasons, requirements, and evidence counts
- `--target-status` is optional:
  - defaults to `promoted` for `dao_ren`, `shu`, and `qi`
  - defaults to `canonical` for `dao_tian`
- Target status must follow existing tier/status policy:
  - `dao_tian`: `canonical | demoted`
  - `dao_ren`: `candidate | promoted | demoted | retired`
  - `shu`: `promoted | demoted | retired`
  - `qi`: `candidate | promoted | demoted | retired`
- Gate policy is deterministic and evidence-count based in P20:
  - `qi -> promoted`: at least 1 supporting ref and 1 verification ref
  - `shu -> promoted`: at least 1 supporting ref and 1 verification ref
  - `dao_ren -> promoted`: at least 2 supporting refs and 1 verification ref
  - `dao_tian -> canonical`: at least 3 supporting refs, at least 2 verification refs, at least 1 teaching ref, reviewer must be provided by `--reviewer`
- Any counterexample ref blocks promotion unless `--allow-counterexamples` is passed
- `--allow-counterexamples` does not make gate automatically pass; it only removes the counterexample hard block
- `--reviewer <text>` is only used for gate evaluation and is not persisted
- Gate validates all role refs:
  - refs must start with `drawer_`
  - refs must exist
  - refs must point to evidence drawers
- Gate does not call LLMs, external research tools, or network services
- Gate does not change `mempal knowledge promote`; P20 is advisory only

## Boundaries

### Allowed Changes
- `src/main.rs`
- `src/core/**`
- `tests/knowledge_lifecycle.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p20-promotion-gate-policy.spec.md`
- `docs/plans/2026-04-25-p20-promotion-gate-policy-implementation.md`

### Forbidden
- Do not add tables, columns, triggers, or migrations
- Do not add Phase-2 `knowledge_cards`
- Do not add MCP / REST gate endpoints
- Do not auto-promote or auto-demote knowledge
- Do not change `mempal_context` response schema or ordering
- Do not change existing `mempal knowledge promote/demote/distill` behavior
- Do not call LLMs or external research tools
- Do not introduce new dependencies

## Out of Scope

- evaluator scoring
- human review UI
- lifecycle event table
- MCP lifecycle or gate tool
- REST lifecycle or gate endpoint
- Phase-2 knowledge cards
- research-rs integration
- field taxonomy management
- dao_tian runtime budget

## Completion Criteria

Scenario: gate allows dao_ren promotion when evidence threshold is met
  Test:
    Filter: test_cli_knowledge_gate_allows_dao_ren_promotion
    Level: integration
  Given a `dao_ren` candidate knowledge drawer exists
  And it has two supporting evidence refs
  And it has one verification evidence ref
  When running `mempal knowledge gate <id>`
  Then the command exits successfully
  And plain output says `allowed=true`
  And the drawer status remains `candidate`
  And no audit entry is written
  And the schema version remains unchanged
  And the vector row for the drawer still exists

Scenario: gate rejects dao_ren promotion when verification is missing
  Test:
    Filter: test_cli_knowledge_gate_rejects_missing_verification
    Level: integration
  Given a `dao_ren` candidate knowledge drawer exists
  And it has two supporting evidence refs
  And it has zero verification refs
  When running `mempal knowledge gate <id> --format json`
  Then the command exits successfully
  And JSON output has `allowed == false`
  And `reasons` includes missing verification evidence

Scenario: gate requires reviewer for dao_tian canonical
  Test:
    Filter: test_cli_knowledge_gate_requires_reviewer_for_dao_tian
    Level: integration
  Given a `dao_tian` canonical knowledge drawer exists
  And it has three supporting evidence refs
  And it has two verification evidence refs
  And it has one teaching evidence ref
  When running `mempal knowledge gate <id>`
  Then JSON or plain report has `allowed == false`
  And `reasons` includes missing reviewer
  When running `mempal knowledge gate <id> --reviewer human --format json`
  Then JSON output has `allowed == true`

Scenario: gate allows shu promotion with one support and one verification
  Test:
    Filter: test_cli_knowledge_gate_allows_shu_promotion
    Level: integration
  Given a `shu` promoted knowledge drawer exists
  And it has one supporting evidence ref
  And it has one verification evidence ref
  When running `mempal knowledge gate <id> --target-status promoted --format json`
  Then JSON output has `allowed == true`
  And JSON output has evidence counts for supporting and verification refs

Scenario: gate blocks counterexamples by default
  Test:
    Filter: test_cli_knowledge_gate_blocks_counterexamples_by_default
    Level: integration
  Given a `qi` candidate knowledge drawer exists
  And it has one supporting evidence ref
  And it has one verification evidence ref
  And it has one counterexample evidence ref
  When running `mempal knowledge gate <id> --format json`
  Then JSON output has `allowed == false`
  And `reasons` includes counterexamples present
  When running `mempal knowledge gate <id> --allow-counterexamples --format json`
  Then JSON output has `allowed == true`

Scenario: gate rejects evidence drawer targets
  Test:
    Filter: test_cli_knowledge_gate_rejects_evidence_drawer
    Level: integration
  Given an evidence drawer exists
  When running `mempal knowledge gate <evidence_id>`
  Then the command fails
  And the error says gate requires a knowledge drawer

Scenario: gate validates role refs are evidence drawers
  Test:
    Filter: test_cli_knowledge_gate_validates_role_refs
    Level: integration
  Given a `dao_ren` candidate knowledge drawer exists
  And one of its supporting refs points to another knowledge drawer
  When running `mempal knowledge gate <id>`
  Then the command fails
  And the error says gate refs must point to evidence drawers

Scenario: gate rejects invalid target status for tier
  Test:
    Filter: test_cli_knowledge_gate_rejects_invalid_target_status
    Level: integration
  Given a `dao_tian` canonical knowledge drawer exists
  When running `mempal knowledge gate <id> --target-status promoted`
  Then the command fails
  And the error says `dao_tian` only allows canonical or demoted
