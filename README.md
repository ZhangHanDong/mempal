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
dimensions = 768  # optional; defaults to 384 when omitted
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
| `mempal cowork-install-hooks [--global-codex]` | Install UserPromptSubmit hooks for Claude Code (+ optional Codex merge) |
| `mempal cowork-drain --target <claude\|codex>` | Drain inbox messages (for hook use; exits 0 on any failure) |
| `mempal cowork-status --cwd <PATH>` | Read-only view of both inboxes at `<PATH>` |
| `mempal fact-check [PATH\|-] [--wing W] [--room R] [--now <UNIX_SECS>]` | Offline contradiction check against KG triples + known entities |
| `mempal bench longmemeval <FILE>` | LongMemEval retrieval benchmark |

## MCP Server (10 tools)

`mempal serve --mcp` exposes these tools via Model Context Protocol:

| Tool | Purpose |
|------|---------|
| `mempal_status` | State + protocol + AAAK spec (teaches agent on first call) |
| `mempal_search` | Hybrid search with tunnel hints, citations, and AAAK-derived structured signals |
| `mempal_ingest` | Store memories with optional importance (0-5) and dry_run; reports `lock_wait_ms` when concurrent ingest was observed |
| `mempal_delete` | Soft-delete with audit trail |
| `mempal_taxonomy` | List or edit routing keywords |
| `mempal_kg` | Knowledge graph: add/query/invalidate/timeline/stats |
| `mempal_tunnels` | Cross-wing room discovery |
| `mempal_peek_partner` | Read partner agent's live session (Claude ↔ Codex), pure read, never writes |
| `mempal_cowork_push` | Send a short handoff message to partner agent's inbox (at-next-submit delivery) |
| `mempal_fact_check` | Offline contradiction detection vs KG triples + known entities (similar-name, relation mismatch, stale facts) |

The server embeds MEMORY_PROTOCOL (11 behavioral rules) in the MCP `initialize.instructions` field. Any MCP client learns the workflow automatically.

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
8. **PARTNER AWARENESS** — use `mempal_peek_partner` for live partner-agent session, not crystallized drawers
9. **DECISION CAPTURE** — `mempal_ingest` is for firm decisions only; include partner input when peek informed the call
10. **COWORK PUSH** — use `mempal_cowork_push` as the SEND primitive in the SEND/READ/PERSIST triad; at-next-submit delivery, not real-time
11. **VERIFY BEFORE INGEST** — call `mempal_fact_check` before persisting a decision that asserts entity relationships; it catches similar-name typos, relation mismatches against the KG, and stale facts with expired `valid_to`

## Agent Cowork (P6 peek + P8 push)

Two coding agents (Claude Code and Codex) can collaborate on the same repo through a per-project inbox + hook-driven injection channel, on top of `mempal_peek_partner` (read live partner session) and `mempal_cowork_push` (send ephemeral handoff).

Install hooks once per repo (run at the repo root):

```bash
mempal cowork-install-hooks --global-codex
```

This writes:

- `.claude/hooks/user-prompt-submit.sh` + merges a registration entry into `.claude/settings.json` so Claude Code fires the hook on every user prompt.
- `~/.codex/hooks.json` UserPromptSubmit entry so Codex fires the same drain on every user prompt.

The `--global-codex` part is optional. The re-run is idempotent and self-heals stale/wrong drain entries — re-installing after a mempal upgrade is always safe.

Delivery is **at-next-UserPromptSubmit**, not real-time: a push from Claude to Codex becomes visible only when the Codex user submits their next prompt, at which point the hook drains the inbox and prepends the message as `additionalContext` on that turn.

Check inbox state at any time without draining:

```bash
mempal cowork-status --cwd "$PWD"
```

### Known limitations

- **Codex feature flag dependency**: Codex's hooks runtime is gated behind the `codex_hooks` feature flag (currently "under development" in shipped `codex-cli`). If the flag is off, Codex silently ignores `~/.codex/hooks.json`. `install-hooks` detects this and prints a warning with the activation command: `codex features enable codex_hooks`.
- **Two Claude-side artifacts**: Claude Code does not auto-discover hook scripts by filename. Both `.claude/hooks/user-prompt-submit.sh` and the matching entry in `.claude/settings.json` are required. `install-hooks` writes both; do not remove either by hand.
- **TUI restart needed after config changes on the Codex side**: Codex reads `config.toml` + `hooks.json` at process startup only. After enabling the feature flag or running `install-hooks`, fully quit and relaunch the Codex TUI before expecting hooks to fire.
- **MCP server re-spawn**: Claude Code spawns the mempal MCP server at client startup. After upgrading the mempal binary (`cargo install ...`), restart Claude Code so the MCP server respawns and exposes newly added tools like `mempal_cowork_push` or `mempal_fact_check`.
- **Bidirectional scope**: `mempal_cowork_push` currently requires an MCP client identifying itself as `claude-code` or `codex` (or their aliases). Generic MCP clients cannot push because caller identity is required to fill the `from` field and enforce self-push rejection. This is by design for the Claude ↔ Codex pair.

## Concurrent Ingest Safety (P9-B)

Two agents writing to the same source simultaneously used to be a TOCTOU race: both would pass the dedup check, both would insert, producing duplicate drawers or mismatched vectors. Since 0.4.0, `mempal_ingest` and `ingest_file_with_options` acquire a per-source advisory lock before entering the dedup + insert critical section.

- Lock files live at `~/.mempal/locks/<16-hex>.lock`, created lazily, released on guard drop.
- 5 s timeout, 50 ms retry + jitter; `LockError::Timeout` surfaces as an `ingest` error.
- Every non-dry-run response carries `lock_wait_ms: Option<u64>` so agents can detect contention.
- Dry-run does not acquire the lock (no writes, no race).
- Unix only in 0.4.0. On Windows the lock path is a no-op fallback; `LockFileEx` support is tracked for a follow-up.

## Offline Fact Checking (P9-A)

`mempal_fact_check` — and its CLI counterpart `mempal fact-check` — compare a text blob against the existing KG `triples` + the entity registry derived from recent drawers. It flags three issue classes, all deterministic and zero-LLM:

| Issue | Trigger |
|-------|---------|
| `SimilarNameConflict` | Text mentions a name within Levenshtein distance ≤ 2 of a known entity, and the names are not equal. |
| `RelationContradiction` | Text asserts a predicate (e.g. `brother_of`) that's in the incompatibility dictionary against an existing KG triple with the same `(subject, object)` endpoints. |
| `StaleFact` | Text asserts a triple whose KG row has `valid_to < now` (Unix seconds). |

Extracted triples today cover three narrow patterns: "X is Y's ROLE", "X works at / for Y", and "X is [the|a|an] ROLE of Y". Unknown sentence shapes are silently ignored, so the tool errs toward under-reporting rather than false positives.

Protocol Rule 11 guides agents to run this before ingesting a decision that asserts entity relationships. See `specs/p9-fact-checker.spec.md` for the full contract.

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
| `mempal-mcp` | MCP server (9 tools) |
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

After changing the embedding model or dimensions, re-embed existing drawers:

```bash
mempal reindex
```

## Docs

- Design: [`docs/specs/2026-04-08-mempal-design.md`](docs/specs/2026-04-08-mempal-design.md)
- Usage guide: [`docs/usage.md`](docs/usage.md)
- AAAK dialect: [`docs/aaak-dialect.md`](docs/aaak-dialect.md)
- Specs (internal agent-spec contracts, on GitHub): <https://github.com/ZhangHanDong/mempal/tree/main/specs>
- Plans (internal implementation plans, on GitHub): <https://github.com/ZhangHanDong/mempal/tree/main/docs/plans>
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
