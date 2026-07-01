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
dimensions = 768  # 可选；省略时默认 384
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
| `mempal cowork-install-hooks [--global-codex]` | 一键安装 Claude Code + Codex 的 UserPromptSubmit hook |
| `mempal cowork-drain --target <claude\|codex>` | 排空 inbox（供 hook 调用，任意错误均 exit 0） |
| `mempal cowork-status --cwd <PATH>` | 只读查看双方 inbox 当前状态 |
| `mempal fact-check [PATH\|-] [--wing W] [--room R] [--now <UNIX_SECS>]` | 离线矛盾检查（对照 KG 三元组 + 已知 entity） |
| `mempal bench longmemeval <FILE>` | LongMemEval 检索 benchmark |

## MCP 服务器（10 个工具）

`mempal serve --mcp` 通过 Model Context Protocol 暴露：

| 工具 | 用途 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec（首次调用即教会 agent） |
| `mempal_search` | 混合检索 + tunnel 提示 + 引用 + P7 的 AAAK 结构化 signals |
| `mempal_ingest` | 存记忆（可选 importance 0-5 + dry_run）；并发写入时返回 `lock_wait_ms` |
| `mempal_delete` | 软删除 + 审计 |
| `mempal_taxonomy` | 路由关键词管理 |
| `mempal_kg` | 知识图谱：add/query/invalidate/timeline/stats |
| `mempal_tunnels` | 跨 Wing room 发现 |
| `mempal_peek_partner` | 读 partner agent 当前 session（Claude ↔ Codex），纯只读 |
| `mempal_cowork_push` | 向 partner agent inbox 投递短消息（at-next-submit 交付） |
| `mempal_fact_check` | 离线矛盾检测（相似名 / 关系对立 / 时态失效，纯本地无 LLM） |

服务器在 MCP `initialize.instructions` 中嵌入 MEMORY_PROTOCOL（11 条行为规则），任何 MCP 客户端自动接收。

## 记忆协议

mempal 通过自描述教 agent 这些规则：

0. **首次设置** — 调用 `mempal_status` 发现 wing 名称
1. **唤醒** — 不同客户端有不同的预加载机制
2. **断言前验证** — 陈述项目事实前先搜索
3. **不确定时查询** — "我们为什么..."、"上次我们..."
3a. **翻译为英文** — 非英文查询先翻译再搜索
4. **决策后保存** — 保存理由，不仅是结果
5. **引用一切** — 引用 drawer_id 和 source_file
5a. **记日记** — session 结束时在 `wing="agent-diary"` 下记行为观察
8. **Partner 感知** — 问 partner 当前状态用 `mempal_peek_partner`（live session），问已沉淀决策才用 `mempal_search`
9. **决策沉淀** — `mempal_ingest` 只存已达成的硬决策；partner 参与时要把 partner 的关键贡献带进 drawer 正文
10. **COWORK PUSH** — `mempal_cowork_push` 是 SEND 原语，at-next-submit 交付、非实时；别用它做该 ingest 的事
11. **入库前校验** — 在 ingest 含 entity 关系断言的决策前调 `mempal_fact_check`，它抓相似名拼错、KG 关系矛盾、以及 `valid_to` 已过期的 stale fact

## 并发 Ingest 安全（P9-B）

两个 agent 同时向同一个 source 写入以前是 TOCTOU race：两边都通过 dedup 检查、都 insert，结果重复 drawer 或 vector 错配。从 0.4.0 起 `mempal_ingest` 和 `ingest_file_with_options` 在进入 dedup + insert 临界区前获取 per-source 建议锁。

- 锁文件 `~/.mempal/locks/<16-hex>.lock`，懒建，guard drop 时 OS 自动释放
- 默认 5s 超时，50ms 重试 + jitter；超时返回 `LockError::Timeout`
- 非 dry-run 的 ingest response 携带 `lock_wait_ms: Option<u64>`，agent 可据此察觉并发
- dry-run 不占锁（零写入，零 race）
- 0.4.0 仅覆盖 Unix（`flock` 内联 extern，不引 libc crate）；Windows 暂为 no-op 占位，后续用 `LockFileEx` 补齐

## 离线事实核查（P9-A）

`mempal_fact_check`（以及 CLI `mempal fact-check`）对一段文本在现有 KG `triples` + 最近 drawer 聚合的 entity registry 上做比对，标记三类确定性问题（不调 LLM、不走网络）：

| 问题类型 | 触发条件 |
|----------|---------|
| `SimilarNameConflict` | 文本中的名字与已知 entity Levenshtein 距离 ≤ 2 且不相等 |
| `RelationContradiction` | 文本断言的 predicate 与 KG 中同 `(subject, object)` 的现存 triple 在不兼容字典中 |
| `StaleFact` | 文本断言的三元组在 KG 中 `valid_to < now`（Unix 秒） |

文本 → 三元组当前支持三种窄模式："X is Y's ROLE"、"X works at / for Y"、"X is [the|a|an] ROLE of Y"。不认得的句型静默返回空，倾向少报不多报。协议 Rule 11 指导 agent 在 ingest 含实体关系断言的决策前跑一次。完整契约见 `specs/p9-fact-checker.spec.md`。

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
- Spec 体系（仓库内部 agent-spec 合约，GitHub 查看）：<https://github.com/ZhangHanDong/mempal/tree/main/specs>
- 实现计划（仓库内部实现计划，GitHub 查看）：<https://github.com/ZhangHanDong/mempal/tree/main/docs/plans>
- Benchmark：[`benchmarks/longmemeval_s_summary.md`](benchmarks/longmemeval_s_summary.md)
