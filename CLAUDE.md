# mempal

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

### 已完成的 Spec（P0-P8）

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

### 当前 Spec

（无，P8 已完成）

### 实现计划

- `docs/plans/2026-04-08-p0-implementation.md` — P0 关键路径（已完成）
- `docs/plans/2026-04-09-p1-p4-implementation.md` — P1-P4（已完成）
- `docs/plans/2026-04-11-p5-implementation.md` — P5（已完成）
- `docs/plans/2026-04-13-p6-implementation.md` — P6（已完成）
- `docs/plans/2026-04-13-p7-implementation.md` — P7（已完成）
- `docs/plans/2026-04-15-p8-implementation.md` — P8（已完成）

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
- **自描述协议**：MEMORY_PROTOCOL 嵌入 MCP ServerInfo.instructions，10 条规则

## MCP 工具（9 个）

| 工具 | 作用 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec |
| `mempal_search` | 混合检索（BM25 + 向量 + RRF + tunnel hints）+ AAAK 结构化 signals（P7） |
| `mempal_ingest` | 写记忆（支持 dry_run） |
| `mempal_delete` | soft-delete（+ audit） |
| `mempal_taxonomy` | Wing/Room 路由关键词管理 |
| `mempal_kg` | 知识图谱三元组（add/query/invalidate） |
| `mempal_tunnels` | 跨 Wing 链接发现 |
| `mempal_peek_partner` | 读 partner agent 当前 session（live，不存储） |
| `mempal_cowork_push` | 主动投递 ephemeral handoff 到 partner inbox（at-next-submit 交付） |

## Workspace 结构

```
crates/
├── mempal-core/      # 数据模型 + SQLite schema v4 + taxonomy + triples
├── mempal-ingest/    # 导入管道
├── mempal-search/    # 混合搜索（BM25+向量+RRF）+ 路由 + tunnel hints
├── mempal-embed/     # 嵌入层（model2vec 默认, ort 可选）
├── mempal-aaak/      # AAAK 编解码（输出侧）
├── mempal-mcp/       # MCP 服务器（7 工具）
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
