spec: task
name: "P14: mind-model runtime context assembler"
inherits: project
tags: [memory, context, mind-model, cli, runtime]
estimate: 1d
---

## Intent

P12/P13 已经让 mempal 能存储 typed evidence / knowledge drawers，并让
knowledge drawer 在 wake-up 中优先使用 `statement`。但当前 runtime 仍没有
一个按 mind-model 组装上下文的入口：agent 只能做普通 search 或 importance
wake-up，无法稳定获得 `dao_tian -> dao_ren -> shu -> qi -> evidence` 的分层
context pack。

本任务实现最小 runtime assembler：新增 `mempal context` CLI 和核心组装逻辑，
让已存的 knowledge/evidence 能按 anchor、tier、status、field/domain 约束进入
agent 上下文。本任务不做 MCP/REST parity，也不做 promotion/demotion lifecycle。

## Decisions

- 新增 CLI：`mempal context <query>`
- `mempal context` 支持最小 flags：
  - `--field <field>`：默认 `general`
  - `--domain <domain>`：默认 `project`
  - `--cwd <path>`：用于推导当前 worktree/repo anchor，默认当前进程 cwd
  - `--format plain|json`：默认 `plain`
  - `--include-evidence`：默认不注入 evidence section
  - `--max-items <n>`：总 item 上限，默认 `12`
- 新增核心模块 `src/context.rs`，暴露内部 request/response 类型：
  - `ContextRequest`
  - `ContextPack`
  - `ContextSection`
  - `ContextItem`
- Tier section order is fixed and observable as `dao_tian -> dao_ren -> shu -> qi -> evidence`
- Anchor 选择和排序固定为：
  - 若请求显式提供 `--cwd`，按该 cwd 推导当前 `worktree` 和 parent `repo`
  - 否则按当前进程 cwd 推导
  - 同 tier 内优先 `worktree`，再当前 `repo`，再 `repo://legacy` fallback，最后 `global`
  - `global` anchor 只匹配 `domain='global'` 的通用知识，以保持 P12 的 `global anchor requires domain=global` invariant
- Knowledge eligibility：
  - 默认只使用 `status in ('canonical', 'promoted')`
  - `candidate`、`demoted`、`retired` 不进入默认 context pack
  - `dao_tian` 仍遵守 P12 规则，只应出现 `canonical` 或 `demoted`
- Text selection：
  - knowledge item 优先使用非空 `statement`
  - knowledge item 缺失 `statement` 时回退 `content`
  - evidence item 使用 `content`
- Evidence section：
  - 默认不输出 evidence
  - 只有传入 `--include-evidence` 时才按 query 补充 evidence section
  - evidence section 只使用 `memory_kind='evidence'` 且匹配 `domain/field/anchor`
- `trigger_hints` 是输出 metadata，不是执行器：
  - context assembler 可以返回 `trigger_hints`
  - 不得直接调用 skill
  - 不得把 `trigger_hints` 当作 hard-coded `skill_id`
- Plain 输出按 section 分组，每条必须带 `drawer_id` 和 `source_file`
- JSON 输出必须包含：
  - `query`
  - `domain`
  - `field`
  - `anchors`
  - `sections`
  - 每个 item 的 `drawer_id`、`source_file`、`text`、`tier/status`、`anchor_kind/anchor_id`
- 本任务复用现有 search / DB 能力；不得改变 BM25 + vector + RRF ranking 语义
- 本任务不 bump schema；schema v7 已具备 P12/P13 所需字段

## Boundaries

### Allowed Changes
- `src/context.rs`
- `src/lib.rs`
- `src/main.rs`
- `src/core/db.rs`
- `src/search/**`
- `tests/context_assembler.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- 不要修改 `drawers` / `drawer_vectors` / `triples` / `tunnels` schema
- 不要新增 MCP tool，例如 `mempal_context`
- 不要新增 REST endpoint
- 不要实现 promote / demote / publish_anchor lifecycle
- 不要集成 `research-rs`
- 不要引入 Phase-2 `knowledge_cards`
- 不要改变 `mempal search` 或 MCP `mempal_search` 的 response contract
- 不要改变现有 BM25 + vector + RRF ranking 算法
- 不要让 context assembler 直接调用任何 skill
- 不要引入新依赖

## Out of Scope

- MCP `mempal_context` 工具
- REST context API
- automatic skill trigger orchestration
- automatic evidence-to-knowledge distillation
- promotion / demotion evaluator gates
- `worktree -> repo -> global` publication API
- Phase-2 `knowledge_cards` / `knowledge_events`
- research-rs ingestion pipeline
- token-budget optimizer beyond `--max-items`

## Completion Criteria

Scenario: context CLI groups knowledge by mind-model tier order
  Test:
    Filter: test_context_groups_knowledge_by_tier_order
    Level: integration
  Given query 为 `"debug failing build"`
  Given 数据库中存在 active knowledge drawers，tier 分别为 `dao_tian`, `dao_ren`, `shu`, `qi`
  And 每条 drawer 都匹配相同 `query/domain/field`
  When 执行 `mempal context "debug failing build" --field software-engineering`
  Then plain 输出按 `dao_tian`, `dao_ren`, `shu`, `qi` 的 section 顺序出现
  And 每条 item 都包含 `drawer_id`
  And 每条 item 都包含 `source_file`

Scenario: context prefers worktree anchor before repo and global inside same tier
  Test:
    Filter: test_context_prefers_worktree_anchor_before_repo_and_global
    Level: integration
  Given query 为 `"local experiment"`
  Given 当前 cwd 可推导出 `worktree` anchor 和 parent `repo` anchor
  And 数据库中同一 tier 下存在 `worktree`, `repo`, `global` 三条匹配 knowledge
  When 执行 `mempal context "local experiment" --cwd <current_worktree>`
  Then 同一 section 内 `worktree` item 排在 `repo` item 前
  And `repo` item 排在 `global` item 前

Scenario: knowledge item uses statement before rationale content
  Test:
    Filter: test_context_knowledge_item_uses_statement_before_content
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在一个 knowledge drawer
  And `statement == "Reproduce before patching."`
  And `content == "Long rationale explaining why reproduction prevents false fixes."`
  When 执行 `mempal context "debug"`
  Then 输出 item text 包含 `"Reproduce before patching."`
  And 输出 item text 不包含完整 rationale body

Scenario: inactive knowledge statuses are excluded from default context
  Test:
    Filter: test_context_excludes_inactive_knowledge_statuses
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在 `candidate`, `demoted`, `retired`, `promoted`, `canonical` 五种 status 的匹配 knowledge
  When 执行 `mempal context "debug"`
  Then 输出包含 `promoted` knowledge
  And 输出包含 `canonical` knowledge
  And 输出不包含 `candidate` knowledge
  And 输出不包含 `demoted` knowledge
  And 输出不包含 `retired` knowledge

Scenario: evidence section is omitted by default
  Test:
    Filter: test_context_omits_evidence_by_default
    Level: integration
  Given query 为 `"observed failure"`
  Given 数据库中存在匹配 query 的 evidence drawer
  When 执行 `mempal context "observed failure"`
  Then 输出不包含 evidence section
  And 输出不包含该 evidence drawer 的 `drawer_id`

Scenario: include-evidence adds evidence section after qi
  Test:
    Filter: test_context_include_evidence_adds_evidence_section_after_qi
    Level: integration
  Given query 为 `"observed failure"`
  Given 数据库中存在匹配 query 的 `qi` knowledge drawer
  And 数据库中存在匹配 query 的 evidence drawer
  When 执行 `mempal context "observed failure" --include-evidence`
  Then 输出包含 `qi` section
  And 输出包含 `evidence` section
  And `evidence` section 排在 `qi` section 之后
  And evidence item 使用 `content` 作为 text

Scenario: json output exposes stable context pack shape
  Test:
    Filter: test_context_json_output_exposes_stable_pack_shape
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在一个带 `trigger_hints` 的 `shu` knowledge drawer
  When 执行 `mempal context "debug" --format json`
  Then stdout 是合法 JSON
  And JSON 顶层包含 `query`, `domain`, `field`, `anchors`, `sections`
  And item 包含 `drawer_id`, `source_file`, `text`, `tier`, `status`, `anchor_kind`, `anchor_id`
  And item 暴露 `trigger_hints`
  And context assembler 没有调用任何 skill

Scenario: domain and field filters exclude unrelated knowledge
  Test:
    Filter: test_context_domain_and_field_filters_exclude_unrelated_knowledge
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在 `domain="skill", field="debugging"` 的 matching knowledge
  And 数据库中存在 `domain="project", field="debugging"` 的 matching knowledge
  And 数据库中存在 `domain="skill", field="writing"` 的 matching knowledge
  When 执行 `mempal context "debug" --domain skill --field debugging`
  Then 输出包含 `domain="skill", field="debugging"` 的 item
  And 输出不包含 `domain="project", field="debugging"` 的 item
  And 输出不包含 `domain="skill", field="writing"` 的 item

Scenario: empty context result exits successfully with explicit empty sections
  Test:
    Filter: test_context_empty_result_exits_successfully
    Level: integration
  Given query 为 `"no such topic"`
  Given 数据库中没有匹配 query/domain/field 的 active knowledge
  When 执行 `mempal context "no such topic" --format json`
  Then exit code 为 0
  And JSON `sections` 是空数组
  And stderr 不包含 error

Scenario: invalid max-items is rejected before search
  Test:
    Filter: test_context_rejects_invalid_max_items
    Level: integration
  Given query 为 `"debug"`
  Given 任意数据库状态
  When 执行 `mempal context "debug" --max-items 0`
  Then exit code 非 0
  And stderr 说明 `--max-items` 必须大于 0
  And 不执行 context assembly

Scenario: core assembler API returns typed context pack
  Test:
    Filter: test_context_assembler_returns_typed_pack
    Level: unit
  Given 一个 `ContextRequest` 包含 `query`, `domain`, `field`, `cwd`, `include_evidence`, `max_items`
  And 数据库中存在至少一个 matching knowledge drawer
  When 调用 `src/context.rs` 的核心组装函数
  Then 返回值类型为 `ContextPack`
  And `ContextPack.sections` 中的元素类型为 `ContextSection`
  And `ContextSection.items` 中的元素类型为 `ContextItem`
  And 该 API 不直接写 stdout 或 stderr

Scenario: context assembler does not bump schema
  Test:
    Filter: test_context_assembler_does_not_bump_schema
    Level: integration
  Given query 为 `"debug"`
  Given 一个 schema v7 数据库
  When 执行 `mempal context "debug"`
  Then schema_version 仍然是 7
  And 不创建新表
  And 不修改 `drawers` 表结构
