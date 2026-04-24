spec: task
name: "P17: bootstrap knowledge lifecycle CLI"
inherits: project
tags: [memory, knowledge, lifecycle, cli]
estimate: 0.5d
---

## Intent

P12-P16 已经把 knowledge drawer、runtime context、MCP context 和 skill guidance 串成闭环，但 knowledge 的 `status` 仍只能在 ingest 时一次性写入。P17 在 bootstrap drawer 架构上补最小 lifecycle CLI：允许人工或 agent 在有证据引用的前提下 promote / demote 现有 knowledge drawer，同时保持 context 只唤醒 active status。

本任务不进入 Phase-2 `knowledge_cards`，不新增 schema，只在现有 drawer metadata 上做受约束的状态变更，并把 reason / reviewer 写入现有 JSONL audit log。

## Decisions

- 新增 CLI command group：`mempal knowledge`
- 新增 `mempal knowledge promote <drawer_id>`：
  - required `--status promoted|canonical`
  - required one or more `--verification-ref <drawer_id>`
  - required `--reason <text>`
  - optional `--reviewer <text>`
  - appends verification refs to `verification_refs` with stable de-duplication
  - updates `status`
- 新增 `mempal knowledge demote <drawer_id>`:
  - required `--status demoted|retired`
  - required one or more `--evidence-ref <drawer_id>`
  - required `--reason <text>`
  - required `--reason-type contradicted|obsolete|superseded|out_of_scope|unsafe`
  - appends evidence refs to `counterexample_refs` with stable de-duplication
  - updates `status`
- Lifecycle commands only accept `memory_kind=knowledge`
- Lifecycle commands reject missing drawer ids instead of creating new drawers
- Lifecycle commands enforce existing tier/status policy:
  - `dao_tian`: `canonical | demoted`
  - `dao_ren`: `candidate | promoted | demoted | retired`
  - `shu`: `promoted | demoted | retired`
  - `qi`: `candidate | promoted | demoted | retired`
- Promotion status is limited to `promoted | canonical`
- Demotion status is limited to `demoted | retired`
- Lifecycle commands do not change content, statement, tier, anchor metadata, vectors, FTS, or schema
- Lifecycle commands append an audit entry with command, drawer_id, old_status, new_status, refs, reason, and reviewer / reason_type

## Boundaries

### Allowed Changes
- `src/core/db.rs`
- `src/main.rs`
- `tests/**`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p17-knowledge-lifecycle.spec.md`
- `docs/plans/2026-04-24-p17-knowledge-lifecycle-implementation.md`

### Forbidden
- Do not add tables, columns, triggers, or migrations
- Do not add a `knowledge_cards` implementation
- Do not add MCP / REST lifecycle endpoints
- Do not change `mempal_context` response schema
- Do not change default context active status rules
- Do not re-embed or mutate vectors
- Do not rewrite source content
- Do not introduce new dependencies

## Out of Scope

- automatic promotion or demotion
- evaluator scoring
- human review UI
- Phase-2 `knowledge_cards`
- lifecycle event table
- MCP lifecycle tool
- REST lifecycle endpoint
- research-rs integration

## Completion Criteria

Scenario: promote candidate knowledge to active context
  Test:
    Filter: test_cli_knowledge_promote_updates_status_and_verification_refs
    Level: integration
  Given a `dao_ren` knowledge drawer with status `candidate`
  And a supporting evidence drawer exists
  When running `mempal knowledge promote <id> --status promoted --verification-ref <evidence_id> --reason "validated in test" --reviewer "human"`
  Then the command exits successfully
  And the drawer status becomes `promoted`
  And `verification_refs` contains `<evidence_id>`
  And default `mempal context` includes the promoted drawer

Scenario: demote active knowledge out of default context
  Test:
    Filter: test_cli_knowledge_demote_updates_status_and_counterexample_refs
    Level: integration
  Given a `shu` knowledge drawer with status `promoted`
  And a counterexample evidence drawer exists
  When running `mempal knowledge demote <id> --status demoted --evidence-ref <evidence_id> --reason "contradicted in test" --reason-type contradicted`
  Then the command exits successfully
  And the drawer status becomes `demoted`
  And `counterexample_refs` contains `<evidence_id>`
  And default `mempal context` excludes the demoted drawer

Scenario: lifecycle rejects evidence drawers
  Test:
    Filter: test_cli_knowledge_lifecycle_rejects_evidence_drawer
    Level: integration
  Given an evidence drawer exists
  When running `mempal knowledge promote <evidence_id> --status promoted --verification-ref drawer_verify --reason "bad"`
  Then the command fails
  And the error says lifecycle requires a knowledge drawer

Scenario: lifecycle enforces tier status policy
  Test:
    Filter: test_cli_knowledge_lifecycle_rejects_invalid_tier_status
    Level: integration
  Given a `dao_tian` knowledge drawer with status `canonical`
  When running `mempal knowledge promote <id> --status promoted --verification-ref drawer_verify --reason "bad"`
  Then the command fails
  And the error says `dao_tian` only allows `canonical` or `demoted`

Scenario: lifecycle writes audit entries
  Test:
    Filter: test_cli_knowledge_lifecycle_writes_audit_entry
    Level: integration
  Given a knowledge drawer exists
  When running a successful lifecycle command with reason and reviewer
  Then `~/.mempal/audit.jsonl` receives one lifecycle entry
  And the entry includes old_status, new_status, refs, reason, and reviewer

Scenario: lifecycle does not bump schema or rewrite vectors
  Test:
    Filter: test_knowledge_lifecycle_does_not_bump_schema_or_rewrite_vectors
    Level: integration
  Given a knowledge drawer exists with an embedding vector
  And the current schema version is recorded
  When running a successful lifecycle command
  Then the schema version remains unchanged
  And the vector row for that drawer still exists
