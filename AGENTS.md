# mempal

本文件是仓库级 agent 工作约束，内容与 `CLAUDE.md` 保持一致；在本仓库内工作的 agent 都应遵循以下规则。

Rust 实现的 coding agent 项目记忆工具。单二进制，`cargo install mempal`，10 秒内带出处找回历史决策。

## Skills

**必须使用项目内的 Rust 技能**：`skills/rust-skills/SKILL.md`

编写、审查、调试、重构 Rust 代码时，遵循该 skill 的四步工作流（理解 → 服从 → 释放 → 约束）和概念锚点框架。

## 参考实现

mempal 借鉴 MemPalace 的设计理念（verbatim 存储、Wing/Room 结构、AAAK 压缩），用 Rust 从零实现并修复其缺陷。以下两个本地项目是关键参考：

- **MemPalace 源码**：`/Users/zhangalex/Work/Projects/AI/mempalace` — Python 原版实现，查看 `mempalace/` 目录下的 searcher.py、palace_graph.py、dialect.py、knowledge_graph.py 等模块了解原始设计
- **MemPalace 书稿**：`/Users/zhangalex/Work/Projects/AI/mempalace-book` — 基于源码的设计分析书，`book/src/` 下 30 章（含 Part 10 mempal Rust 重铸）+ 4 个附录

实现时遇到设计疑问，优先查阅书稿中的分析（特别是附录 C 的 AAAK 评估和附录 A/B 的 E2E Trace），而非直接复制 Python 代码。

## 设计文档

`docs/specs/2026-04-08-mempal-design.md` — 完整架构设计，所有实现必须以此为准。

## Spec 体系

项目使用 agent-spec 管理任务合约。所有实现必须对照 spec 验收。

### 项目级 Spec
- `specs/project.spec.md` — 项目约束（edition、依赖、编码规范、架构不变量）

### 已完成的 Spec（P0-P22）

| Spec | 状态 | 范围 |
|------|------|------|
| `specs/p0-core-scaffold.spec.md` | 完成 | workspace 骨架 + SQLite schema |
| `specs/p0-embed-trait.spec.md` | 完成 | Embedder trait（model2vec 默认 + ort 可选） |
| `specs/p0-ingest.spec.md` | 完成 | 导入管道（格式检测/归一化/分块/存储） |
| `specs/p0-search-cli.spec.md` | 完成 | 搜索引擎 + CLI |
| `specs/p1-routing-citation.spec.md` | 完成 | 查询路由 + 引用组装 |
| `specs/p2-mcp.spec.md` | 完成 | MCP 服务器（7 工具） |
| `specs/p3-aaak.spec.md` | 完成 | AAAK 编解码（BNF + 往返验证） |
| `specs/p4-rest-api.spec.md` | 完成 | REST API（feature-gated） |
| `specs/p5-wake-up-importance.spec.md` | 完成 | L1 重要性排序 wake-up（schema v4） |
| `specs/p5-kg-timeline-stats.spec.md` | 完成 | KG timeline + stats actions |
| `specs/p5-semantic-dedup.spec.md` | 完成 | 语义去重检测（ingest warning） |
| `specs/p5-agent-diary.spec.md` | 完成 | Agent 日记 convention（协议层） |
| `specs/p5-format-support.spec.md` | 完成 | Slack DM + Codex CLI 格式支持 |
| `specs/p6-cowork-peek-and-decide.spec.md` | 完成 | Claude↔Codex 协作：live session peek（`mempal_peek_partner`）+ Rule 8/9 |
| `specs/p7-search-structured-signals.spec.md` | 完成 | `mempal_search` 响应每条结果附带 5 个 AAAK-derived 结构化字段（`entities` / `topics` / `flags` / `emotions` / `importance_stars`），`content` 保持 raw |
| `specs/p8-cowork-inbox-push.spec.md` | 完成 | 双向 cowork push — `mempal_cowork_push` MCP 工具 + `cowork-drain` / `cowork-status` / `cowork-install-hooks` CLI + 对称 UserPromptSubmit hook 注入（at-next-submit 交付） |
| `specs/p9-fact-checker.spec.md` | 完成 | 离线事实核查 — `mempal_fact_check` MCP 工具 + `fact-check` CLI，基于 KG triples + 已知 entity 检测 SimilarNameConflict / RelationContradiction / StaleFact（协议 Rule 11） |
| `specs/p9-ingest-lock.spec.md` | 完成 | Per-source `flock` 锁 — 消除 Claude↔Codex 并发 ingest 同一 source 的 TOCTOU race；`IngestStats` / `IngestResponse.lock_wait_ms` 提供并发等待可观测性 |
| `specs/p10-explicit-tunnels.spec.md` | 完成 | schema v6 + `mempal_tunnels` 扩 add/list/delete/follow 显式跨 wing 链接 |
| `specs/p10-normalize-version.spec.md` | 完成 | schema v7 `normalize_version` 列 + `reindex --stale` 机制 |
| `specs/p11-transcript-noise-strip.spec.md` | 完成 | Claude/Codex transcript noise strip + `CURRENT_NORMALIZE_VERSION=2` |
| `specs/p11-chunk-neighbors.spec.md` | 完成 | `mempal_search` / CLI search 可选返回命中 chunk 前后邻居 |
| `specs/p11-diary-daily-rollup.spec.md` | 完成 | `agent-diary` 天粒度 upsert drawer，防 chatty agent 爆炸 |
| `specs/p12-mind-model-bootstrap.spec.md` | 完成 | Stage-1 mind-model bootstrap：typed drawers + `dao/shu/qi` 最小治理字段 + `global/repo/worktree` anchor metadata |
| `specs/p13-wake-up-statement.spec.md` | 完成 | wake-up 最小闭环：knowledge drawer 优先按 `statement` 唤醒，evidence 继续按 `content` 唤醒 |
| `specs/p13-ingest-identity.spec.md` | 完成 | typed/bootstrap ingest `drawer_id` identity parity：MCP / REST / 文件入口统一使用 bootstrap identity components |
| `specs/p14-context-assembler.spec.md` | 完成 | mind-model runtime assembler：`mempal context` 按 `dao_tian -> dao_ren -> shu -> qi -> evidence` 和 `worktree -> repo -> global` 组装 context pack |
| `specs/p15-mcp-context.spec.md` | 完成 | `mempal_context` MCP 工具：向 agent runtime 暴露 P14 mind-model context pack |
| `specs/p16-context-skill-guidance.spec.md` | 完成 | context-guided skill selection protocol：`mempal_context` 辅助 workflow/skill/tool 选择，但 `trigger_hints` 只做 bias、不自动执行 |
| `specs/p17-knowledge-lifecycle.spec.md` | 完成 | bootstrap knowledge lifecycle CLI：`mempal knowledge promote/demote` 受约束更新 knowledge drawer status 与 refs，并写 audit |
| `specs/p18-knowledge-distill.spec.md` | 完成 | bootstrap knowledge distill CLI：`mempal knowledge distill` 从 evidence refs 创建 candidate knowledge drawer |
| `specs/p19-lifecycle-ref-validation.spec.md` | 完成 | lifecycle evidence ref hardening：`promote/demote` refs 必须是存在的 evidence drawers |
| `specs/p20-promotion-gate-policy.spec.md` | 完成 | read-only promotion gate policy：`mempal knowledge gate` 评估 knowledge drawer 是否满足最小提升门槛 |
| `specs/p21-mcp-knowledge-gate.spec.md` | 完成 | `mempal_knowledge_gate` MCP 工具：向 agent runtime 暴露 P20 read-only promotion gate |
| `specs/p22-mcp-knowledge-distill.spec.md` | 完成 | `mempal_knowledge_distill` MCP 工具：从 evidence refs 创建 candidate knowledge drawer |

### 当前 Spec（草稿，未实现）

暂无。

### 实现计划

- `docs/plans/2026-04-08-p0-implementation.md` — P0 关键路径（已完成）
- `docs/plans/2026-04-09-p1-p4-implementation.md` — P1-P4（已完成）
- `docs/plans/2026-04-11-p5-implementation.md` — P5（已完成）
- `docs/plans/2026-04-13-p6-implementation.md` — P6（已完成）
- `docs/plans/2026-04-13-p7-implementation.md` — P7（已完成）
- `docs/plans/2026-04-15-p8-implementation.md` — P8（已完成）
- `docs/plans/2026-04-17-p9-implementation.md` — P9 fact-checker + ingest-lock（已完成）
- `docs/plans/2026-04-23-p10-explicit-tunnels-implementation.md` — P10 explicit tunnels（已完成）
- `docs/plans/2026-04-23-p10-normalize-version-implementation.md` — P10 normalize-version（已完成）
- `docs/plans/2026-04-23-p11-transcript-noise-strip-implementation.md` — P11 transcript noise strip（已完成）
- `docs/plans/2026-04-24-p11-chunk-neighbors-implementation.md` — P11 chunk neighbors（已完成）
- `docs/plans/2026-04-24-p11-diary-daily-rollup-implementation.md` — P11 diary daily rollup（已完成）
- `docs/plans/2026-04-21-p12-implementation.md` — P12 mind-model bootstrap（已完成）
- `docs/plans/2026-04-23-p13a-implementation.md` — P13A wake-up statement（已完成）
- `docs/plans/2026-04-23-p13b-implementation.md` — P13B bootstrap ingest identity parity（已完成）
- `docs/plans/2026-04-24-p14-context-assembler-implementation.md` — P14 mind-model runtime context assembler（已完成）
- `docs/plans/2026-04-24-p15-mcp-context-implementation.md` — P15 mempal_context MCP tool（已完成）
- `docs/plans/2026-04-24-p16-context-skill-guidance-implementation.md` — P16 context-guided skill selection protocol（已完成）
- `docs/plans/2026-04-24-p17-knowledge-lifecycle-implementation.md` — P17 bootstrap knowledge lifecycle CLI（已完成）
- `docs/plans/2026-04-24-p18-knowledge-distill-implementation.md` — P18 bootstrap knowledge distill CLI（已完成）
- `docs/plans/2026-04-24-p19-lifecycle-ref-validation-implementation.md` — P19 lifecycle evidence ref validation（已完成）
- `docs/plans/2026-04-25-p20-promotion-gate-policy-implementation.md` — P20 promotion gate policy（已完成）
- `docs/plans/2026-04-25-p21-mcp-knowledge-gate-implementation.md` — P21 MCP knowledge gate（已完成）
- `docs/plans/2026-04-25-p22-mcp-knowledge-distill-implementation.md` — P22 MCP knowledge distill（已完成）

### Spec 使用方式

```bash
agent-spec parse specs/p6-cowork-peek-and-decide.spec.md
agent-spec lint specs/p6-cowork-peek-and-decide.spec.md --min-score 0.7
```

## 关键架构约束

- **存储**：SQLite + sqlite-vec，单文件 `~/.mempal/palace.db`，schema v4
- **嵌入**：model2vec-rs 默认（potion-multilingual-128M, 256d），可选 ort (ONNX) 通过 `onnx` feature flag
- **搜索**：BM25 (FTS5) + 向量 + RRF 融合混合检索
- **AAAK 是输出格式化器**：不被 ingest 或 search 依赖
- **数据永远 raw 存储**：drawers 表存原文，向量索引在 drawer_vectors 表（维度动态）
- **搜索结果强制带引用**：`SearchResult` 包含 `source_file`、`drawer_id`、`tunnel_hints`
- **知识图谱**：triples 表已激活（手动 CRUD），支持时态验证
- **隧道**：动态跨 Wing 链接发现，内联到搜索结果
- **自描述协议**：MEMORY_PROTOCOL 嵌入 MCP ServerInfo.instructions，13 条规则

## MCP 工具（13 个）

| 工具 | 作用 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec |
| `mempal_search` | 混合检索（BM25 + 向量 + RRF + tunnel hints）+ AAAK 结构化 signals（P7） |
| `mempal_context` | mind-model runtime context：按 `dao_tian -> dao_ren -> shu -> qi` 组装指导性 context pack；用于辅助 workflow/skill/tool 选择但不自动执行（P15/P16） |
| `mempal_knowledge_distill` | 从 existing evidence drawer refs 创建 candidate `dao_ren` / `qi` knowledge drawer（P22） |
| `mempal_knowledge_gate` | read-only promotion readiness check：评估 knowledge drawer 是否满足提升门槛（P21） |
| `mempal_ingest` | 写记忆（支持 dry_run；P9-B 暴露 `lock_wait_ms`） |
| `mempal_delete` | soft-delete（+ audit） |
| `mempal_taxonomy` | Wing/Room 路由关键词管理 |
| `mempal_kg` | 知识图谱三元组（add/query/invalidate） |
| `mempal_tunnels` | 跨 Wing 链接发现 |
| `mempal_peek_partner` | 读 partner agent 当前 session（live，不存储） |
| `mempal_cowork_push` | 主动投递 ephemeral handoff 到 partner inbox（at-next-submit 交付） |
| `mempal_fact_check` | 离线矛盾检测（SimilarNameConflict / RelationContradiction / StaleFact）—— P9 |

## mempal 检索纪律

当 agent 回答本项目的历史决策、实现细节、bug 成因、架构理由、或“为什么/怎么工作”类问题时：

1. 每个 session 先调一次 `mempal_status`，再决定是否使用 `wing` / `room` filter。
2. 对项目事实优先使用 `mempal_search`，不要只靠 repo grep、当前对话记忆或常识猜测。
3. 在本仓库内，只要 `mempal_status` 已确认存在 `wing="mempal"`，默认先用 `wing="mempal"` 缩小范围；只有用户明确要求跨项目 / 全局搜索时才放宽。
4. 历史决策、设计理由、bug 成因这类问题，第一轮检索默认使用简短英文语义 query，且 `top_k=2`；只有证据不足时才逐步放大到 `top_k=3-4` 或放宽 query。
5. 如果 MCP 客户端提示 Large response / Large MCP response，优先重试更窄的 query、加 `wing` / `room`、或降低 `top_k`；不要直接消费一大段 raw `content`。
6. 显式消费 `mempal_search` 返回的结构化 signals，而不是只读 `content`：
   - 决策问题：优先 `flags` 包含 `DECISION` 的结果
   - 实现 / bug / 架构问题：优先 `flags` 包含 `TECHNICAL` 的结果
   - 同等条件下优先处理 `importance_stars` 更高的结果
   - 用 `entities` 和 `topics` 缩小歧义结果集
7. 将 `content` 视为 raw text；不要期待或解析 `mempal_search` 返回 AAAK 格式文本。
8. 基于 mempal 结果作答时，必须引用 `drawer_id` 和 `source_file`。
9. 如果没有找到高信号结果，要明确说明“没找到足够证据”，然后扩大查询范围；不要猜。

## Workspace 结构

```
crates/
├── mempal-core/      # 数据模型 + SQLite schema v4 + taxonomy + triples
├── mempal-ingest/    # 导入管道
├── mempal-search/    # 混合搜索（BM25+向量+RRF）+ 路由 + tunnel hints
├── mempal-embed/     # 嵌入层（model2vec 默认, ort 可选）
├── mempal-aaak/      # AAAK 编解码（输出侧）
├── mempal-mcp/       # MCP 服务器（9 工具）
├── mempal-api/       # REST API（feature-gated）
└── mempal-cli/       # CLI 入口（含 reindex, kg, tunnels）
```

## 代码规范

- Edition 2024
- `#![warn(clippy::all)]`
- 错误处理：`anyhow`（应用层）+ `thiserror`（库层）
- 异步：`tokio`，features=["full"]
- 不用 `.unwrap()`，用 `?` 或 `.expect("reason")`
- 每次 commit 后：调 `mempal_ingest` 存决策记忆（Rule 4）

## Auto-Dream 集成

当 Claude Code 执行 auto-dream 或手动 dream 时：

1. **验证**：调 `mempal_search` 核实正在整理的事实是否与 mempal 记忆一致
2. **保存**：将整理后的高价值洞察存到 mempal（`mempal_ingest`，importance >= 3）
3. **解矛盾**：MEMORY.md 与 mempal drawer 矛盾时，以 mempal 为准（mempal 有出处引用）
4. **写日记**：将 dream 摘要存为 agent diary（`wing="agent-diary"`, `room="claude"`）
5. **清理 KG**：检查 triples 中是否有过期关系需要 invalidate

Dream 是 mempal 的"REM 睡眠"——短期 session 记忆被整理为长期项目记忆。

## Known Limitations / Operational Notes

以下是 0.3.0 已知、来自真实 E2E 的跨系统约束，和 mempal 代码无关但会直接影响使用体验，记录在此避免重复踩坑：

1. **`mempal cowork-install-hooks` 写两件 Claude 侧制品**：`.claude/hooks/user-prompt-submit.sh`（脚本）+ `.claude/settings.json` 下的 `hooks.UserPromptSubmit` 条目（注册）。Claude Code 不按文件名约定自动发现脚本，两件**都必须有**hook 才会 fire。`install-hooks` 已自动处理 + 自愈 stale 条目；请勿手工移除其中任一。
2. **Codex 侧依赖 `codex_hooks` feature flag**：shipped `codex-cli ≤ 0.120.0` 该 flag 处于 "under development" 且默认 `false`，此时 Codex runtime 完全忽略 `~/.codex/hooks.json`。`install-hooks` 检测到会打印 warning + 激活命令 `codex features enable codex_hooks`。
3. **Codex TUI 进程启动时一次性缓存 config**：改完 `config.toml` 或 `hooks.json`（含 feature flag / install-hooks）后，必须完全退出并重启 Codex TUI；已在跑的进程拿不到新配置。
4. **Claude Code 的 MCP server 是 session startup spawn**：`cargo install` 升级 mempal binary 后，Claude Code 还在用旧 MCP server 进程，**不认识新加的工具**（如 `mempal_cowork_push`）。升级后重启 Claude Code，MCP server 会 respawn 到新 binary。
5. **`mempal_cowork_push` 依赖 MCP `ClientInfo.name` 被识别为 Claude/Codex 家族之一**（`src/cowork/peek.rs` `Tool::from_str_ci`）：caller_tool 推断基于 MCP `ClientInfo.name`，是 self-push 拒绝和 `InboxMessage.from` 填写的前提。当前识别名单（0.3.1）：`claude` / `claude-code` / `claude_code` / `codex` / `codex-cli` / `codex_cli` / `codex-tui` / `codex-mcp-client`（Codex 实际发送的字符串，源自 `codex-rs/codex-mcp/src/mcp_connection_manager.rs:1458`）。其它 MCP 客户端名字不在此列表时即使显式传 `target_tool` 也会被拒；这不是 by-design scope 限制，只是当前只覆盖 Claude↔Codex pair，遇到新家族继续扩名单即可。
