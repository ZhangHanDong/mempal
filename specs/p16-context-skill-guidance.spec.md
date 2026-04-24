spec: task
name: "P16: context-guided skill selection protocol"
inherits: project
tags: [memory, context, skill, protocol]
estimate: 0.25d
---

## Intent

P15 已经把 `mempal_context` 暴露给 MCP agent，但 agent 还需要明确知道如何把 mind-model context 用到 skill 选择上。P16 固化运行时纪律：agent 应先消费 `dao_tian` / `dao_ren` 的判断框架，再用 `shu` / `qi` 和 `trigger_hints` 偏置 workflow / skill / tool 选择，但 mempal 绝不自动执行 skill，也不覆盖 system / user / repo instructions。

本任务只更新自描述协议和文档，并用协议测试锁住不变量；不新增 MCP/CLI/REST 行为。

## Decisions

- `MEMORY_PROTOCOL` 必须明确 `mempal_context` 的 skill-selection 消费顺序：
  - 先读 `dao_tian`
  - 再读 `dao_ren`
  - 再用 `shu` 选择 workflow / skill
  - 最后用 `qi` 选择 concrete tool / command
- `trigger_hints` 只允许作为 bias metadata：
  - 可以影响候选 workflow / skill / tool
  - 不得作为 hard-coded skill id
  - 不得自动调用或执行任何 skill
- memory hints 的优先级低于：
  - system instructions
  - user instructions
  - repo instructions such as `AGENTS.md` / `CLAUDE.md`
  - client-native skill availability
- 当 `mempal_context` 和 `mempal_search` 都适用时：
  - `mempal_context` 用于 choosing an approach / workflow / skill
  - `mempal_search` 用于 verifying project facts / decisions / citations
- 不改变 `mempal_context` response schema
- 不新增 skill registry、skill execution engine、hook injection 或 prompt rewriting
- 不 bump schema

## Boundaries

### Allowed Changes
- `src/core/protocol.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p16-context-skill-guidance.spec.md`
- `docs/plans/2026-04-24-p16-context-skill-guidance-implementation.md`

### Forbidden
- 不要修改 `src/context.rs`
- 不要修改 `src/mcp/server.rs`
- 不要修改 `src/mcp/tools.rs`
- 不要修改 search / ingest / schema / migrations
- 不要新增 MCP tool、CLI command 或 REST endpoint
- 不要实现 automatic skill trigger orchestration
- 不要让 `trigger_hints` 变成 hard-coded skill id
- 不要引入新依赖

## Out of Scope

- skill execution engine
- skill registry / installer integration
- automatic prompt injection
- runtime hook integration
- Phase-2 knowledge lifecycle
- research-rs integration
- context pack schema changes

## Completion Criteria

Scenario: protocol explains context before skill selection
  Test:
    Filter: contains_context_before_skill_selection_guidance
    Level: unit
  Given `MEMORY_PROTOCOL` is embedded in MCP status instructions
  When an agent reads the protocol
  Then it instructs the agent to call `mempal_context` before choosing a workflow or skill
  And it states the order `dao_tian -> dao_ren -> shu -> qi`

Scenario: protocol keeps trigger hints non-executable
  Test:
    Filter: contains_trigger_hints_bias_not_execution_guidance
    Level: unit
  Given `MEMORY_PROTOCOL` is embedded in MCP status instructions
  When an agent reads the `trigger_hints` guidance
  Then it states `trigger_hints` are bias metadata only
  And it states they are not hard-coded skill ids
  And it states they must not execute skills

Scenario: protocol preserves instruction precedence
  Test:
    Filter: contains_memory_hints_instruction_precedence
    Level: unit
  Given `MEMORY_PROTOCOL` is embedded in MCP status instructions
  When memory hints conflict with system, user, repo, or client skill availability
  Then the protocol says those instructions and availability take precedence over memory hints

Scenario: conflicting memory hints do not authorize automatic skill execution
  Test:
    Filter: contains_conflicting_hints_do_not_authorize_execution
    Level: unit
  Given a context item contains `trigger_hints`
  And the hinted workflow conflicts with system, user, repo, or client-native skill rules
  When an agent reads the protocol
  Then it must follow the higher-priority instruction source
  And it must not automatically execute the hinted skill

Scenario: protocol distinguishes context from search
  Test:
    Filter: contains_context_vs_search_responsibility_split
    Level: unit
  Given both `mempal_context` and `mempal_search` exist
  When an agent needs approach guidance
  Then the protocol assigns workflow / skill choice to `mempal_context`
  And the protocol assigns project fact verification and citations to `mempal_search`

Scenario: protocol update does not bump schema
  Test:
    Filter: protocol_update_does_not_change_schema_version
    Level: unit
  Given the current database schema version before P16
  When P16 updates only protocol guidance
  Then the schema version remains unchanged
