# mempal

Project memory for coding agents. Single binary, `cargo install mempal`, find past decisions with citations in seconds.

## What It Does

```
Agent writes code → commits → mempal saves the decision context
Next session (any agent) → mempal search → finds the decision with source citation
```

- **Hybrid search**: BM25 keyword matching + vector semantic search, merged via Reciprocal Rank Fusion
- **Knowledge graph**: subject-predicate-object triples with temporal validity (valid_from/valid_to)
- **Cross-project tunnels**: automatic discovery when the same room appears in multiple wings
- **Self-describing protocol**: MEMORY_PROTOCOL embedded in MCP ServerInfo teaches any agent how to use mempal — no system prompt configuration required
- **Multilingual**: model2vec-rs (BGE-M3 distilled) as default embedder, zero native dependencies
- **Single file**: everything lives in `~/.mempal/palace.db` (SQLite + sqlite-vec)

## Quick Start

```bash
cargo install --path crates/mempal-cli --locked

mempal init ~/code/myapp
mempal ingest ~/code/myapp --wing myapp
mempal search "auth decision clerk"
mempal wake-up
```

With REST support:

```bash
cargo install --path crates/mempal-cli --locked --features rest
```

## Configuration

Config at `~/.mempal/config.toml` (optional, defaults work without it):

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "model2vec"                          # default, zero native deps
# model = "minishlab/potion-multilingual-128M" # default multilingual model (1024d)
```

Other backends:

```toml
# Local ONNX (requires --features onnx)
[embed]
backend = "onnx"

# External API
[embed]
backend = "api"
api_endpoint = "http://localhost:11434/api/embeddings"
api_model = "nomic-embed-text"
```

## Commands

| Command | Purpose |
|---------|---------|
| `mempal init <DIR> [--dry-run]` | Infer wing/rooms from project tree |
| `mempal ingest <DIR> --wing <W> [--dry-run]` | Chunk, embed, and store |
| `mempal search <QUERY> [--wing W] [--room R] [--json]` | Hybrid search (BM25 + vector + RRF) |
| `mempal wake-up [--format aaak]` | Context refresh, sorted by importance |
| `mempal compress <TEXT>` | AAAK format output |
| `mempal delete <DRAWER_ID>` | Soft-delete a drawer |
| `mempal purge [--before TIMESTAMP]` | Permanently remove soft-deleted drawers |
| `mempal kg add <S> <P> <O>` | Add a knowledge graph triple |
| `mempal kg query [--subject S] [--predicate P]` | Query triples |
| `mempal kg timeline <ENTITY>` | Chronological view of an entity |
| `mempal kg stats` | Knowledge graph statistics |
| `mempal tunnels` | Cross-wing room links |
| `mempal taxonomy list / edit` | Manage routing keywords |
| `mempal reindex` | Re-embed all drawers after model change |
| `mempal status` | DB stats, schema version, scopes |
| `mempal serve [--mcp]` | MCP server (+ REST with feature) |
| `mempal bench longmemeval <FILE>` | LongMemEval retrieval benchmark |

## MCP Server (7 tools)

`mempal serve --mcp` exposes these tools via Model Context Protocol:

| Tool | Purpose |
|------|---------|
| `mempal_status` | State + protocol + AAAK spec (teaches agent on first call) |
| `mempal_search` | Hybrid search with tunnel hints and citations |
| `mempal_ingest` | Store memories with optional importance (0-5) and dry_run |
| `mempal_delete` | Soft-delete with audit trail |
| `mempal_taxonomy` | List or edit routing keywords |
| `mempal_kg` | Knowledge graph: add/query/invalidate/timeline/stats |
| `mempal_tunnels` | Cross-wing room discovery |

The server embeds MEMORY_PROTOCOL (9 behavioral rules) in the MCP `initialize.instructions` field. Any MCP client learns the workflow automatically.

## Memory Protocol

mempal teaches agents these rules through self-description:

0. **FIRST-TIME SETUP** — call `mempal_status` to discover wings before filtering
1. **WAKE UP** — different clients have different pre-load mechanisms
2. **VERIFY BEFORE ASSERTING** — search before stating project facts
3. **QUERY WHEN UNCERTAIN** — search on "why did we...", "last time we..."
3a. **TRANSLATE TO ENGLISH** — translate non-English queries before searching
4. **SAVE AFTER DECISIONS** — persist rationale, not just outcomes
5. **CITE EVERYTHING** — reference drawer_id and source_file
5a. **KEEP A DIARY** — record behavioral observations in wing="agent-diary"

## Search Architecture

```
query → BM25 (FTS5)     → ranked by keyword match
      → Vector (sqlite-vec) → ranked by semantic similarity
      → RRF Fusion (k=60)   → merged ranking
      → Wing/Room filter     → scoped results
      → Tunnel hints         → cross-project references
```

## Knowledge Graph

```bash
mempal kg add "Kai" "recommends" "Clerk"
mempal kg add "Clerk" "replaced" "Auth0" --source-drawer drawer_xxx
mempal kg timeline "Kai"
mempal kg stats
```

Triples support temporal validity — relationships can be invalidated when they expire.

## Agent Diary

Cross-session behavioral learning — agents record observations, lessons, and patterns:

```bash
# Search diary entries
mempal search "lesson" --wing agent-diary
mempal search "pattern" --wing agent-diary --room claude
```

Diary entries use the existing `mempal_ingest` tool with `wing="agent-diary"` and `room=agent-name`. MEMORY_PROTOCOL Rule 5a teaches agents to write diary entries. Integrates with Claude Code's auto-dream for automatic memory consolidation.

## Ingest Formats (5)

| Format | Auto-detected by |
|--------|-----------------|
| Claude Code JSONL | `type` + `message` fields |
| ChatGPT JSON | Array or `mapping` tree |
| Codex CLI JSONL | `session_meta` + `event_msg` entries |
| Slack DM JSON | `type: "message"` + `user` + `text` |
| Plain text | Fallback |

## AAAK Compression

Output-only format readable by any LLM without decoding:

```bash
mempal compress "Kai recommended Clerk over Auth0 based on pricing and DX"
# V1|manual|compress|1744156800|cli
# 0:KAI+CLK+AUT|kai_clerk_auth0|"Kai recommended Clerk over Auth0..."|★★★★|determ|DECISION
```

Chinese text uses jieba-rs POS tagging for proper word segmentation.

## Architecture

| Crate | Responsibility |
|-------|---------------|
| `mempal-core` | Types, SQLite schema v4, taxonomy, triples |
| `mempal-embed` | Embedder trait (model2vec default, ort optional) |
| `mempal-ingest` | Format detection, normalization, chunking (5 formats) |
| `mempal-search` | Hybrid search (BM25 + vector + RRF), routing, tunnels |
| `mempal-aaak` | AAAK encode/decode with BNF grammar + roundtrip tests |
| `mempal-mcp` | MCP server (7 tools) |
| `mempal-api` | Feature-gated REST API |
| `mempal-cli` | CLI entrypoint |

Key design choices:
- **model2vec-rs** default embedder — zero native deps, multilingual (BGE-M3 distilled)
- **ort (ONNX)** available behind `onnx` feature flag for max quality
- **FTS5** for BM25 keyword search — synced via SQLite triggers
- **Soft-delete** with audit trail — `mempal delete` + `mempal purge`
- **Importance ranking** — drawers have 0-5 importance, wake-up sorts by importance
- **Semantic dedup** — ingest warns (doesn't block) when similar content exists

## Development

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```

After changing the embedding model, re-embed existing drawers:

```bash
mempal reindex
```

## Docs

- Design: [`docs/specs/2026-04-08-mempal-design.md`](docs/specs/2026-04-08-mempal-design.md)
- Usage guide: [`docs/usage.md`](docs/usage.md)
- AAAK dialect: [`docs/aaak-dialect.md`](docs/aaak-dialect.md)
- Specs: [`specs/`](specs)
- Plans: [`docs/plans/`](docs/plans)
- Benchmark: [`benchmarks/longmemeval_s_summary.md`](benchmarks/longmemeval_s_summary.md) — includes the older 384d baseline and the newer model2vec 256d run

## Book: MemPalace — Reforging Memory in Rust

mempal 的设计分析和完整技术叙事，收录在《MemPalace: AI 记忆的第一性原理》Part 10（第 26-30 章）：

- [中文版](https://zhanghandong.github.io/mempalace-book/ch26-why-rewrite-in-rust.html)
- [English](https://zhanghandong.github.io/mempalace-book/en/ch26-why-rewrite-in-rust.html)

| 章节 | 内容 |
|------|------|
| 第 26 章 | 为什么用 Rust 重铸 — 触发点、重写判断、语言选择 |
| 第 27 章 | 保留了什么、改变了什么 — 5 维度对比 + 架构图 |
| 第 28 章 | 自描述协议 — MEMORY_PROTOCOL、7 条规则、agent 生命周期 |
| 第 29 章 | 多 Agent 协作 — Claude↔Codex 接力、反模式发现、agent 日记 |
| 第 30 章 | 诚实的差距 — benchmark 数据、6 个 gap |
