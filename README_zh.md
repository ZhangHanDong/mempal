# mempal

Coding agent 的项目记忆工具。单二进制，`cargo install mempal`，10 秒内带出处找回历史决策。

## 做什么

```
Agent 写代码 → 提交 → mempal 保存决策上下文
下一个 session（任何 agent）→ mempal search → 带出处找回决策
```

- **混合检索**：BM25 关键词匹配 + 向量语义搜索，通过 RRF（Reciprocal Rank Fusion）融合
- **知识图谱**：subject-predicate-object 三元组，支持时态验证（valid_from/valid_to）
- **跨项目隧道**：自动发现多个 Wing 中同名 Room 的链接
- **自描述协议**：MEMORY_PROTOCOL 嵌入 MCP ServerInfo，任何 agent 连接后自动学会使用方式——无需系统提示配置
- **多语言**：model2vec-rs（BGE-M3 蒸馏）作为默认嵌入器，零原生依赖
- **单文件**：所有数据在 `~/.mempal/palace.db`（SQLite + sqlite-vec）

## 快速开始

```bash
cargo install --path crates/mempal-cli --locked

mempal init ~/code/myapp
mempal ingest ~/code/myapp --wing myapp
mempal search "auth decision clerk"
mempal wake-up
```

启用 REST 支持：

```bash
cargo install --path crates/mempal-cli --locked --features rest
```

## 配置

配置文件 `~/.mempal/config.toml`（可选，不存在时使用默认值）：

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "model2vec"                          # 默认，零原生依赖
# model = "minishlab/potion-multilingual-128M" # 默认多语言模型
```

其他后端：

```toml
# 本地 ONNX（需要 --features onnx）
[embed]
backend = "onnx"

# 外部 API
[embed]
backend = "api"
api_endpoint = "http://localhost:11434/api/embeddings"
api_model = "nomic-embed-text"
```

## 命令一览

| 命令 | 用途 |
|------|------|
| `mempal init <DIR> [--dry-run]` | 从项目目录推断 wing/room |
| `mempal ingest <DIR> --wing <W> [--dry-run]` | 分块、嵌入、存储 |
| `mempal search <QUERY> [--wing W] [--room R] [--json]` | 混合检索（BM25 + 向量 + RRF） |
| `mempal wake-up [--format aaak]` | 上下文刷新，按重要性排序 |
| `mempal compress <TEXT>` | AAAK 格式输出 |
| `mempal delete <DRAWER_ID>` | 软删除 |
| `mempal purge [--before TIMESTAMP]` | 永久清除已软删除的记忆 |
| `mempal kg add <S> <P> <O>` | 添加知识图谱三元组 |
| `mempal kg query [--subject S] [--predicate P]` | 查询三元组 |
| `mempal kg timeline <ENTITY>` | 实体的时间线视图 |
| `mempal kg stats` | 知识图谱统计 |
| `mempal tunnels` | 跨 Wing room 链接 |
| `mempal taxonomy list / edit` | 管理路由关键词 |
| `mempal reindex` | 切换模型后重新嵌入所有 drawer |
| `mempal status` | 数据库统计、schema 版本、scope 分布 |
| `mempal serve [--mcp]` | MCP 服务器（+ REST） |
| `mempal bench longmemeval <FILE>` | LongMemEval 检索 benchmark |

## MCP 服务器（7 个工具）

`mempal serve --mcp` 通过 Model Context Protocol 暴露：

| 工具 | 用途 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec（首次调用即教会 agent） |
| `mempal_search` | 混合检索 + tunnel 提示 + 引用 |
| `mempal_ingest` | 存记忆（可选 importance 0-5 + dry_run） |
| `mempal_delete` | 软删除 + 审计 |
| `mempal_taxonomy` | 路由关键词管理 |
| `mempal_kg` | 知识图谱：add/query/invalidate/timeline/stats |
| `mempal_tunnels` | 跨 Wing room 发现 |

服务器在 MCP `initialize.instructions` 中嵌入 MEMORY_PROTOCOL（9 条行为规则），任何 MCP 客户端自动接收。

## 记忆协议

mempal 通过自描述教 agent 这些规则：

0. **首次设置** — 调用 `mempal_status` 发现 wing 名称
1. **唤醒** — 不同客户端有不同的预加载机制
2. **断言前验证** — 陈述项目事实前先搜索
3. **不确定时查询** — "我们为什么..."、"上次我们..."
3a. **翻译为英文** — 非英文查询先翻译再搜索
4. **决策后保存** — 保存理由，不仅是结果
5. **引用一切** — 引用 drawer_id 和 source_file
5a. **记日记** — 在 wing="agent-diary" 记录行为观察

## 检索架构

```
query → BM25 (FTS5)         → 关键词排序
      → Vector (sqlite-vec) → 语义相似度排序
      → RRF 融合 (k=60)     → 合并排序
      → Wing/Room 过滤      → 范围限定
      → Tunnel 提示         → 跨项目引用
```

## 知识图谱

```bash
mempal kg add "Kai" "recommends" "Clerk"
mempal kg add "Clerk" "replaced" "Auth0" --source-drawer drawer_xxx
mempal kg timeline "Kai"
mempal kg stats
```

三元组支持时态验证——关系过期后可标记为无效。

## Agent 日记

跨 session 行为学习——agent 记录观察、教训和模式：

```bash
# 搜索日记
mempal search "lesson" --wing agent-diary
mempal search "pattern" --wing agent-diary --room claude
```

日记通过现有的 `mempal_ingest` 工具写入，`wing="agent-diary"`，`room=agent 名字`。MEMORY_PROTOCOL Rule 5a 教 agent 在 session 结束时写日记。可与 Claude Code 的 auto-dream 集成，实现自动记忆整理。

## 导入格式（5 种）

| 格式 | 自动检测方式 |
|------|------------|
| Claude Code JSONL | `type` + `message` 字段 |
| ChatGPT JSON | 数组或 `mapping` 树 |
| Codex CLI JSONL | `session_meta` + `event_msg` 条目 |
| Slack DM JSON | `type: "message"` + `user` + `text` |
| 纯文本 | 兜底 |

## AAAK 压缩

输出格式化器，任何 LLM 无需解码即可阅读：

```bash
mempal compress "Kai recommended Clerk over Auth0 based on pricing and DX"
# V1|manual|compress|1744156800|cli
# 0:KAI+CLK+AUT|kai_clerk_auth0|"Kai recommended Clerk over Auth0..."|★★★★|determ|DECISION
```

中文文本使用 jieba-rs 词性标注进行分词。

## 架构

| Crate | 职责 |
|-------|------|
| `mempal-core` | 类型、SQLite schema v4、taxonomy、triples |
| `mempal-embed` | Embedder trait（model2vec 默认，ort 可选） |
| `mempal-ingest` | 格式检测、归一化、分块（5 种格式） |
| `mempal-search` | 混合检索（BM25 + 向量 + RRF）、路由、tunnel |
| `mempal-aaak` | AAAK 编解码（BNF 语法 + 往返验证） |
| `mempal-mcp` | MCP 服务器（7 工具） |
| `mempal-api` | REST API（feature-gated） |
| `mempal-cli` | CLI 入口 |

关键设计：
- **model2vec-rs** 默认嵌入——零原生依赖，多语言（BGE-M3 蒸馏）
- **ort (ONNX)** 通过 `onnx` feature flag 可选启用
- **FTS5** BM25 关键词搜索——通过 SQLite 触发器同步
- **软删除** + 审计日志——`mempal delete` + `mempal purge`
- **重要性排序**——drawer 有 0-5 重要性评分，wake-up 按重要性排序
- **语义去重**——ingest 时检测相似内容，warning 但不阻塞

## 开发

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

切换嵌入模型后重建向量：

```bash
mempal reindex
```

## 文档

- 设计文档：[`docs/specs/2026-04-08-mempal-design.md`](docs/specs/2026-04-08-mempal-design.md)
- 使用指南：[`docs/usage.md`](docs/usage.md)
- AAAK 方言：[`docs/aaak-dialect.md`](docs/aaak-dialect.md)
- Spec 体系：[`specs/`](specs)
- 实现计划：[`docs/plans/`](docs/plans)
- Benchmark：[`benchmarks/longmemeval_s_summary.md`](benchmarks/longmemeval_s_summary.md)
