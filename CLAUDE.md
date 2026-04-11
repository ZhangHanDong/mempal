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

### 已完成的 Spec（P0-P4）

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

### 当前 Spec（P5 — MemPalace 借鉴改进）

| Spec | 范围 | 优先级 | 估时 |
|------|------|--------|------|
| `specs/p5-wake-up-importance.spec.md` | L1 重要性排序 wake-up（schema v4） | P0 | 1d |
| `specs/p5-kg-timeline-stats.spec.md` | KG timeline + stats actions | P1 | 0.5d |
| `specs/p5-semantic-dedup.spec.md` | 语义去重检测（ingest warning） | P1 | 0.5d |
| `specs/p5-agent-diary.spec.md` | Agent 日记 convention（协议层） | P2 | 0.5d |
| `specs/p5-format-support.spec.md` | Slack DM + Codex CLI 格式支持 | P2 | 1d |

### 实现计划

- `docs/plans/2026-04-08-p0-implementation.md` — P0 关键路径（已完成）
- `docs/plans/2026-04-09-p1-p4-implementation.md` — P1-P4（已完成）
- `docs/plans/2026-04-11-p5-implementation.md` — **P5 当前计划**（5 tasks, 3.5d）

**开始 P5 实现时**：先读对应的 spec，再按 plan 的 Task 步骤执行。Task 1 必须先做（schema v4），然后 2+3 可并行，4+5 可并行。

### Spec 使用方式

```bash
agent-spec parse specs/p5-wake-up-importance.spec.md
agent-spec lint specs/p5-wake-up-importance.spec.md --min-score 0.7
```

## 关键架构约束

- **存储**：SQLite + sqlite-vec，单文件 `~/.mempal/palace.db`，schema v3
- **嵌入**：model2vec-rs 默认（potion-multilingual-128M, 256d），可选 ort (ONNX) 通过 `onnx` feature flag
- **搜索**：BM25 (FTS5) + 向量 + RRF 融合混合检索
- **AAAK 是输出格式化器**：不被 ingest 或 search 依赖
- **数据永远 raw 存储**：drawers 表存原文，向量索引在 drawer_vectors 表（维度动态）
- **搜索结果强制带引用**：`SearchResult` 包含 `source_file`、`drawer_id`、`tunnel_hints`
- **知识图谱**：triples 表已激活（手动 CRUD），支持时态验证
- **隧道**：动态跨 Wing 链接发现，内联到搜索结果
- **自描述协议**：MEMORY_PROTOCOL 嵌入 MCP ServerInfo.instructions，7 条规则

## MCP 工具（7 个）

| 工具 | 作用 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec |
| `mempal_search` | 混合检索（BM25 + 向量 + RRF + tunnel hints） |
| `mempal_ingest` | 写记忆（支持 dry_run） |
| `mempal_delete` | soft-delete（+ audit） |
| `mempal_taxonomy` | Wing/Room 路由关键词管理 |
| `mempal_kg` | 知识图谱三元组（add/query/invalidate） |
| `mempal_tunnels` | 跨 Wing 链接发现 |

## Workspace 结构

```
crates/
├── mempal-core/      # 数据模型 + SQLite schema v3 + taxonomy + triples
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
