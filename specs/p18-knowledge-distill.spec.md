spec: task
name: "P18: bootstrap knowledge distill CLI"
inherits: project
tags: [memory, knowledge, distill, cli]
estimate: 0.5d
---

## Intent

P17 补齐了 `promote` / `demote`，但 Phase-1 mind model 仍缺少显式 `distill`：从已有 evidence drawers 创建 candidate knowledge drawer。P18 新增 `mempal knowledge distill`，让人类或 agent 可以在引用证据的前提下生成候选知识，但不自动提升、不自动评价、不进入 Phase-2 `knowledge_cards`。

本任务复用现有 drawer bootstrap identity、typed knowledge metadata、embedding、audit log 和 context 规则。

## Decisions

- 新增 CLI：`mempal knowledge distill`
- Required arguments:
  - `--statement <text>`
  - `--content <text>`
  - `--tier dao_ren|qi`
  - one or more `--supporting-ref <drawer_id>`
- Optional arguments:
  - `--wing <wing>` default `mempal`
  - `--room <room>` default `knowledge`
  - `--domain project|agent|skill|global` default `project`
  - `--field <field>` default `general`
  - `--cwd <path>` for deriving worktree/repo anchor
  - `--scope-constraints <text>`
  - repeated `--counterexample-ref <drawer_id>`
  - repeated `--teaching-ref <drawer_id>`
  - repeated `--intent-tag <text>`
  - repeated `--workflow-bias <text>`
  - repeated `--tool-need <text>`
  - `--importance <0-5>` default `2`
  - `--dry-run`
- Distill always creates `memory_kind=knowledge`
- Distill always sets `status=candidate`
- Distill only allows `tier=dao_ren|qi` because current P12 tier/status policy does not allow candidate `dao_tian` or candidate `shu`
- Distill validates all role refs:
  - each ref must look like a drawer id
  - each ref must exist
  - `supporting_refs` must be non-empty
- Distill derives anchor from `--cwd` when provided, otherwise current process cwd
- Distill uses existing bootstrap identity components so equivalent distill requests produce the same drawer_id
- Distill writes `source_file=knowledge://...` using existing knowledge source URI format
- Distill embeds `content` and inserts one vector unless `--dry-run`
- Distill is idempotent:
  - if the computed drawer already exists, it does not insert a duplicate
  - output still reports the existing drawer_id
- Distill appends an audit entry on successful non-dry-run create
- Distill does not call LLMs, summarize evidence, score confidence, promote, demote, or create Phase-2 knowledge cards
- No MCP / REST endpoint in P18
- No schema bump

## Boundaries

### Allowed Changes
- `src/main.rs`
- `tests/**`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p18-knowledge-distill.spec.md`
- `docs/plans/2026-04-24-p18-knowledge-distill-implementation.md`

### Forbidden
- Do not add tables, columns, triggers, or migrations
- Do not add Phase-2 `knowledge_cards`
- Do not add MCP / REST distill endpoints
- Do not change `mempal_context` response schema or ordering
- Do not change search ranking
- Do not auto-promote distilled knowledge
- Do not call an LLM or external research tool
- Do not introduce new dependencies

## Out of Scope

- automatic evidence summarization
- evaluator scoring
- promotion gates
- human review UI
- Phase-2 knowledge cards
- MCP lifecycle/distill tools
- REST lifecycle/distill endpoints
- research-rs integration

## Completion Criteria

Scenario: distill creates candidate knowledge from evidence
  Test:
    Filter: test_cli_knowledge_distill_creates_candidate_knowledge
    Level: integration
  Given an evidence drawer exists
  When running `mempal knowledge distill --statement "Prefer evidence first" --content "Use cited evidence before asserting project facts." --tier dao_ren --supporting-ref <evidence_id>`
  Then the command exits successfully
  And a knowledge drawer is inserted
  And `status == candidate`
  And `supporting_refs` contains `<evidence_id>`
  And default `mempal context` does not include the candidate drawer

Scenario: distill dry-run returns deterministic drawer id without writing
  Test:
    Filter: test_cli_knowledge_distill_dry_run_no_write
    Level: integration
  Given an evidence drawer exists
  When running the same `mempal knowledge distill ... --dry-run` command twice
  Then both outputs contain the same drawer_id
  And the database drawer count is unchanged

Scenario: distill rejects unsupported candidate tier
  Test:
    Filter: test_cli_knowledge_distill_rejects_dao_tian_candidate
    Level: integration
  Given an evidence drawer exists
  When running `mempal knowledge distill --tier dao_tian`
  Then the command fails
  And the error says distill only allows candidate `dao_ren` or `qi`

Scenario: distill rejects missing or nonexistent evidence refs
  Test:
    Filter: test_cli_knowledge_distill_rejects_missing_supporting_refs
    Level: integration
  Given no supporting refs are provided
  When running `mempal knowledge distill`
  Then clap rejects the command before writing
  And no knowledge drawer is created

Scenario: distill stores trigger hints as bias metadata
  Test:
    Filter: test_cli_knowledge_distill_stores_trigger_hints
    Level: integration
  Given an evidence drawer exists
  When running distill with `--intent-tag debugging --workflow-bias reproduce-first --tool-need cargo-test`
  Then the inserted knowledge drawer has `trigger_hints`
  And the trigger hints contain those three values

Scenario: distill writes audit and preserves schema
  Test:
    Filter: test_cli_knowledge_distill_writes_audit_and_preserves_schema
    Level: integration
  Given an evidence drawer exists
  And the current schema version is recorded
  When running a successful non-dry-run distill command
  Then `~/.mempal/audit.jsonl` receives a `knowledge_distill` entry
  And the schema version remains unchanged
