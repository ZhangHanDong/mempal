spec: task
name: "P15: mempal_context MCP tool"
inherits: project
tags: [memory, context, mind-model, mcp]
estimate: 0.5d
---

## Intent

P14 已经实现 CLI-first `mempal context` 和核心 `assemble_context`。但 agent runtime 主要通过 MCP 使用 mempal；如果没有 MCP surface，agent 仍然只能调用 `mempal_search`，无法直接获得按 `dao_tian -> dao_ren -> shu -> qi -> evidence` 组装的 mind-model context pack。

本任务新增 `mempal_context` MCP 工具，复用 P14 assembler，不改变 CLI、search、schema 或 context 排序规则。

## Decisions

- 新增 MCP tool：`mempal_context`
- Request 字段与 P14 CLI 对齐：
  - `query: String`
  - `field: Option<String>`，默认 `general`
  - `domain: Option<String>`，默认 `project`
  - `cwd: Option<String>`，默认当前进程 cwd
  - `include_evidence: Option<bool>`，默认 `false`
  - `max_items: Option<usize>`，默认 `12`
- Response 是 machine-readable context pack：
  - `query`
  - `domain`
  - `field`
  - `anchors`
  - `sections`
  - 每个 item 包含 `drawer_id`, `source_file`, `text`, `tier`, `status`, `anchor_kind`, `anchor_id`, optional `parent_anchor_id`, optional `trigger_hints`
- `mempal_context` 必须复用 `src/context.rs::assemble_context`
- `max_items == 0` 返回 MCP invalid params
- unsupported `domain` 返回 MCP invalid params
- invalid `cwd` / anchor derivation failure 返回 MCP invalid params
- embedder/search/db failures 返回 MCP internal error
- `mempal_context` 是纯 read；不得写 drawer、vector、triple、tunnel、inbox 或 audit log
- 不改变 `mempal_search` request/response contract
- 不新增 CLI flags
- 不 bump schema

## Boundaries

### Allowed Changes
- `src/mcp/tools.rs`
- `src/mcp/server.rs`
- `src/core/protocol.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `tests/**`

### Forbidden
- 不要修改 `src/context.rs` 的 assembly rules，除非只是为了 MCP 复用暴露必要 helper
- 不要修改 `drawers` / `drawer_vectors` / `triples` / `tunnels` schema
- 不要新增 REST endpoint
- 不要改变 `mempal search` 或 MCP `mempal_search`
- 不要实现 skill trigger execution
- 不要实现 promote / demote / publish_anchor lifecycle
- 不要集成 `research-rs`
- 不要引入新依赖

## Out of Scope

- REST `context` API
- automatic skill trigger orchestration
- runtime injection hooks
- token-budget optimizer beyond `max_items`
- evidence-to-knowledge distillation
- Phase-2 `knowledge_cards`
- research-rs pipeline

## Completion Criteria

Scenario: MCP context returns tier-ordered knowledge sections
  Test:
    Filter: test_mcp_context_returns_tier_ordered_sections
    Level: integration
  Given 数据库中存在 active knowledge drawers，tier 分别为 `dao_tian`, `dao_ren`, `shu`, `qi`
  And 每条 drawer 都匹配 query `"debug failing build"`
  When MCP 客户端调用 `mempal_context(query="debug failing build")`
  Then response.sections 按 `dao_tian`, `dao_ren`, `shu`, `qi` 顺序出现
  And 每条 item 都包含 `drawer_id`
  And 每条 item 都包含 `source_file`

Scenario: MCP context defaults match CLI context defaults
  Test:
    Filter: test_mcp_context_defaults_match_cli_context_defaults
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在一个 matching promoted `shu` knowledge drawer
  When MCP 客户端调用 `mempal_context` 只传 `query`
  Then response.domain == `"project"`
  And response.field == `"general"`
  And response.sections 不包含 `evidence`
  And response.anchors 非空
  And response.sections 包含该 `shu` item

Scenario: MCP context include_evidence appends evidence section
  Test:
    Filter: test_mcp_context_include_evidence_appends_evidence_section
    Level: integration
  Given query 为 `"observed failure"`
  Given 数据库中存在 matching `qi` knowledge drawer
  And 数据库中存在 matching evidence drawer
  When MCP 客户端调用 `mempal_context(query="observed failure", include_evidence=true)`
  Then response.sections 包含 `qi`
  And response.sections 包含 `evidence`
  And `evidence` section 排在 `qi` section 后

Scenario: MCP context rejects max_items zero
  Test:
    Filter: test_mcp_context_rejects_max_items_zero
    Level: integration
  Given query 为 `"debug"`
  Given 任意数据库状态
  When MCP 客户端调用 `mempal_context(query="debug", max_items=0)`
  Then 返回 MCP invalid params error
  And 不执行 context assembly

Scenario: MCP context rejects unsupported domain
  Test:
    Filter: test_mcp_context_rejects_unsupported_domain
    Level: integration
  Given query 为 `"debug"`
  And domain 为 `"invalid"`
  Given 任意数据库状态
  When MCP 客户端调用 `mempal_context(query="debug", domain="invalid")`
  Then 返回 MCP invalid params error

Scenario: MCP context has no database side effects
  Test:
    Filter: test_mcp_context_has_no_db_side_effects
    Level: integration
  Given query 为 `"debug"`
  Given 数据库中存在 matching knowledge drawer
  And 记录调用前 schema_version, drawer_count, triple_count, taxonomy_count, scope_counts
  When MCP 客户端连续调用 `mempal_context` 三次
  Then 调用后的 schema_version, drawer_count, triple_count, taxonomy_count, scope_counts 与调用前完全一致
  And MCP `mempal_search` request/response contract 不变

Scenario: MCP tool registry includes mempal_context
  Test:
    Filter: test_mcp_tool_registry_includes_mempal_context
    Level: unit
  Given MCP server tool registry 已生成
  When 枚举工具名称
  Then 包含 `mempal_context`
  And `mempal_context` description 提到 `dao_tian -> dao_ren -> shu -> qi`
