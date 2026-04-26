spec: task
name: "P26: dao_tian runtime context budget"
inherits: project
tags: [memory, context, mind-model, runtime, mcp]
---

## Intent

`dao_tian` 是跨领域高层原则，数量应该极少，且不应在每次 runtime context
assembly 中压过领域规则和可执行方法。P14/P15 已经实现
`dao_tian -> dao_ren -> shu -> qi` 的固定排序，但还没有单独限制
`dao_tian` 的默认注入数量。

本任务给 `mempal context` 和 `mempal_context` 增加保守的 `dao_tian`
运行时预算：默认最多 1 条，可由调用方显式禁用或提高；总输出仍受
`max_items` 控制。

## Decisions

- `mempal context` 新增 `--dao-tian-limit <N>`。
- `mempal_context` 新增可选参数 `dao_tian_limit`。
- 默认 `dao_tian_limit=1`。
- `dao_tian_limit=0` 表示不输出 `dao_tian` section。
- `dao_tian_limit=N` 表示 `dao_tian` section 最多输出 N 条 active knowledge items。
- `max_items` 仍是全局总预算；`dao_tian_limit` 不能让总条目数超过 `max_items`。
- 非 `dao_tian` tiers 继续共享剩余 `max_items` 预算，不新增单独 tier limit。
- Section order 仍为 `dao_tian -> dao_ren -> shu -> qi -> evidence`。
- 如果 `dao_tian_limit=0` 或没有匹配项，则不输出空的 `dao_tian` section。
- `dao_tian_limit=0` 是合法值；`max_items=0` 仍必须拒绝。

## Acceptance Criteria

Scenario: default context caps dao_tian to one item
  Test:
    Package: mempal
    Filter: test_context_default_caps_dao_tian_to_one
  Given 数据库中存在两条 active `dao_tian` knowledge drawers 和一条 active `dao_ren`
  When 调用 core context assembler 且使用默认 `dao_tian_limit`
  Then `dao_tian` section 最多包含 1 条 item
  And `dao_ren` 仍可使用剩余总预算进入 context

Scenario: CLI can disable dao_tian injection
  Test:
    Package: mempal
    Filter: test_cli_context_dao_tian_limit_zero_omits_section
  Given 数据库中存在 active `dao_tian` 和 active `dao_ren`
  When 执行 `mempal context "debug" --dao-tian-limit 0 --format json`
  Then JSON response 不包含 `dao_tian` section
  And response 仍可包含 `dao_ren` section

Scenario: CLI can raise dao_tian budget
  Test:
    Package: mempal
    Filter: test_cli_context_dao_tian_limit_two_allows_two_items
  Given 数据库中存在两条 active `dao_tian`
  When 执行 `mempal context "debug" --dao-tian-limit 2 --format json`
  Then `dao_tian` section 包含 2 条 item

Scenario: MCP can disable dao_tian injection
  Test:
    Package: mempal
    Filter: test_mcp_context_dao_tian_limit_zero_omits_section
  Given 数据库中存在 active `dao_tian` 和 active `shu`
  When MCP 客户端调用 `mempal_context(query="debug", dao_tian_limit=0)`
  Then response 不包含 `dao_tian` section
  And response 仍可包含 `shu` section

Scenario: max_items remains the global cap
  Test:
    Package: mempal
    Filter: test_context_max_items_still_caps_raised_dao_tian_limit
  Given 数据库中存在多条 active `dao_tian`
  When 调用 context assembler，设置 `max_items=1` 且 `dao_tian_limit=2`
  Then 总输出 item 数最多为 1

Scenario: max_items zero remains invalid
  Test:
    Package: mempal
    Filter: test_context_rejects_invalid_max_items
  Given 任意数据库状态
  When 执行 `mempal context "debug" --max-items 0`
  Then command fails with a `max-items` validation error

## Out of Scope

- Do not change database schema.
- Do not change search ranking, vector embeddings, or RRF behavior.
- Do not add per-tier budgets for `dao_ren`, `shu`, or `qi`.
- Do not implement automatic `dao_tian` relevance evaluation.
- Do not make wake-up use `dao_tian`; this task only affects `mempal context` and `mempal_context`.
- Do not execute skills or tools based on `dao_tian`.

## Constraints

- Keep `src/context.rs` as the source of truth for budget enforcement.
- Keep CLI and MCP defaults aligned.
- Update protocol and docs so agents know `dao_tian` is sparse by default.
