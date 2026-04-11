# mempal Usage Guide

This guide is for the repository as it exists today: local CLI workflows, MCP usage, AAAK output, the optional REST server, and the native LongMemEval harness.

`mempal` is a local memory system for coding agents. It stores raw text in SQLite, builds embeddings for retrieval, and always returns citations such as `drawer_id` and `source_file`.

## Mental Model

Before using the CLI, keep four nouns straight:

- `wing`: the top-level scope, usually one project or knowledge domain
- `room`: a sub-scope inside a wing, usually inferred from directory names or edited by taxonomy
- `drawer`: one stored memory item or chunk
- `source_file`: where the drawer came from; for directory ingest, stored relative to the ingest root

`mempal` is raw-first:

- original text lives in the `drawers` table
- vectors live in `drawer_vectors`
- AAAK is output-only and does not replace stored raw text

## Install

Install the CLI locally:

```bash
cargo install --path crates/mempal-cli --locked
```

Install with REST support:

```bash
cargo install --path crates/mempal-cli --locked --features rest
```

For development without installation:

```bash
cargo run -p mempal-cli -- --help
cargo run -p mempal-cli --features rest -- serve --help
```

## Configuration

Config file path:

```text
~/.mempal/config.toml
```

Default config:

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "onnx"
```

Use an external embedding API instead of local ONNX:

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "api"
api_endpoint = "http://localhost:11434/api/embeddings"
api_model = "nomic-embed-text"
```

Notes:

- ONNX is the default backend.
- First ONNX use downloads `all-MiniLM-L6-v2` model assets.
- If `config.toml` is missing, `mempal` still works with defaults.
- The benchmark and search commands use whatever embedder backend is configured here.

## Command Cheat Sheet

Use this when you already know the concepts and just need the right command quickly.

| Command | Purpose |
|---------|---------|
| `mempal init <DIR>` | infer a `wing` and seed initial taxonomy rooms from a project tree |
| `mempal ingest --wing <WING> <DIR>` | chunk, embed, and store a project tree |
| `mempal search <QUERY>` | search drawers, with optional `--wing` and `--room` filters |
| `mempal wake-up` | generate a compact context refresh for an agent |
| `mempal compress <TEXT>` | format arbitrary text as AAAK |
| `mempal taxonomy list` | inspect current routing keywords |
| `mempal taxonomy edit <WING> <ROOM> --keywords ...` | tune routing behavior |
| `mempal status` | inspect DB size, counts, schema version, and deleted drawers |
| `mempal delete <DRAWER_ID>` | soft-delete one drawer |
| `mempal purge [--before ...]` | permanently remove soft-deleted drawers |
| `mempal serve --mcp` | run the MCP server over stdio |
| `mempal bench longmemeval <DATA_FILE>` | run the native LongMemEval retrieval benchmark |

## First 5 Minutes

This is the shortest realistic flow for a new project.

### 1. Inspect the inferred taxonomy

Preview which `wing` and `room` names `mempal` will infer:

```bash
mempal init ~/code/myapp --dry-run
```

Typical output:

```text
dry_run=true
wing: myapp
rooms:
- auth
- deploy
- docs
```

Write those taxonomy entries:

```bash
mempal init ~/code/myapp
```

### 2. Preview ingest before writing

```bash
mempal ingest ~/code/myapp --wing myapp --dry-run
```

Typical output:

```text
dry_run=true files=12 chunks=34 skipped=2
```

This reads, normalizes, chunks, and counts, but does not write drawers or vectors.

### 3. Ingest the project

```bash
mempal ingest ~/code/myapp --wing myapp
```

Optional explicit format selector:

```bash
mempal ingest ~/code/myapp --wing myapp --format convos
```

Every ingest appends a JSONL audit record to:

```text
~/.mempal/audit.jsonl
```

### 4. Search

```bash
mempal search "auth decision clerk"
```

Structured JSON output:

```bash
mempal search "auth decision clerk" --json
```

Restrict to a wing:

```bash
mempal search "database decision" --wing myapp
```

Restrict to a wing and room:

```bash
mempal search "token refresh bug" --wing myapp --room auth
```

### 5. Generate a context refresh

```bash
mempal wake-up
```

Compact AAAK-formatted refresh:

```bash
mempal wake-up --format aaak
```

## Core Workflows

### Search

What a search result includes:

- `drawer_id`
- `content`
- `wing`
- `room`
- `source_file`
- `similarity`
- `route`

`route` explains whether the query used explicit filters or taxonomy routing.

`source_file` is stored relative to the ingest root, so citations stay stable whether the project was ingested via an absolute or relative path.

If you care about deterministic scope, pass `--wing` and optionally `--room` explicitly instead of relying on routing.

### Wake-Up and AAAK

`wake-up` emits a short memory summary for agent context refresh:

```bash
mempal wake-up
```

AAAK output:

```bash
mempal wake-up --format aaak
mempal compress "Kai recommended Clerk over Auth0 based on pricing and DX"
```

Example AAAK output:

```text
V1|manual|compress|1744156800|cli
0:KAI+CLK+AUT|kai_clerk_auth0|"Kai recommended Clerk over Auth0 based on pricing and DX"|★★★★|determ|DECISION
```

AAAK is an output formatter only:

- it does not affect how drawers are stored
- it is not required for ingest or search
- benchmark `--mode aaak` means "index AAAK-formatted retrieval text", not "change the storage layer"

### Chinese Text

AAAK supports Chinese and mixed Chinese-English text:

```bash
mempal compress "张三推荐Clerk替换Auth0，因为价格更优"
```

Chinese entities and topics are extracted with `jieba-rs` POS tagging. People, places, organizations, and content words are turned into entity/topic fields before AAAK formatting.

For the full format specification, see [`docs/aaak-dialect.md`](aaak-dialect.md).

### Taxonomy

List taxonomy entries:

```bash
mempal taxonomy list
```

Edit or add taxonomy keywords:

```bash
mempal taxonomy edit myapp auth --keywords "auth,login,clerk"
```

Use taxonomy when:

- you want routing to pick the right room more reliably
- your repo directory layout is not enough
- you want search behavior to reflect domain language instead of folder names

### Status

Show storage stats:

```bash
mempal status
```

The command reports:

- `schema_version`
- `drawer_count`
- `deleted_drawers` when soft-deleted content exists
- `taxonomy_entries`
- DB file size
- per-`wing` and per-`room` counts

Schema version is backed by SQLite `PRAGMA user_version`. On open, `mempal` applies bundled forward migrations needed to bring an older local database up to the current binary's schema.

### Delete and Purge

These are destructive operations. Use them carefully.

Soft-delete one drawer:

```bash
mempal delete drawer_myapp_auth_1234abcd
```

Current behavior:

- looks up the drawer first
- soft-deletes it
- prints a short summary of what was deleted
- writes an audit log entry
- does not permanently remove it yet

Permanent removal:

```bash
mempal purge
```

Purge only drawers soft-deleted before an ISO timestamp:

```bash
mempal purge --before 2026-04-10T00:00:00Z
```

Important:

- `delete` is reversible only until `purge` runs
- `status` will tell you when deleted drawers are waiting to be purged

## Common Recipes

### Index a repo and search one subsystem

```bash
mempal init ~/code/myapp
mempal ingest ~/code/myapp --wing myapp
mempal search "token refresh bug" --wing myapp --room auth
```

### Preview a large ingest before committing disk and compute

```bash
mempal init ~/code/monorepo --dry-run
mempal ingest ~/code/monorepo --wing monorepo --dry-run
```

### Tune routing when search keeps landing in the wrong room

```bash
mempal taxonomy list
mempal taxonomy edit myapp deploy --keywords "render,railway,postgres,migration"
mempal search "postgres migration" --wing myapp
```

### Refresh an AI agent before continuing work

```bash
mempal wake-up
mempal wake-up --format aaak
```

### Run a fast benchmark sample instead of the full dataset

```bash
mempal bench longmemeval /path/to/longmemeval_s_cleaned.json \
  --limit 20 \
  --out benchmarks/results_longmemeval_20.jsonl
```

## MCP Server

Run stdio MCP explicitly:

```bash
mempal serve --mcp
```

If `mempal` was built without the `rest` feature, plain `mempal serve` behaves the same way.

The MCP server exposes five tools:

- `mempal_status`
- `mempal_search`
- `mempal_ingest`
- `mempal_delete`
- `mempal_taxonomy`

Example request shapes:

```json
{
  "query": "auth decision clerk",
  "wing": "myapp",
  "room": "auth",
  "top_k": 5
}
```

```json
{
  "content": "decided to use Clerk for auth",
  "wing": "myapp",
  "room": "auth",
  "source": "/repo/README.md",
  "dry_run": false
}
```

Preview an ingest without writing (returns the predicted `drawer_id`):

```json
{
  "content": "decided to use Clerk for auth",
  "wing": "myapp",
  "dry_run": true
}
```

Soft-delete a drawer:

```json
{
  "drawer_id": "drawer_myapp_auth_1234abcd"
}
```

```json
{
  "action": "edit",
  "wing": "myapp",
  "room": "auth",
  "keywords": ["auth", "login", "clerk"]
}
```

`mempal_status` also returns the self-describing memory protocol and a dynamically generated AAAK spec so AI clients can learn the tool without a hardcoded prompt.

## REST Server

Build with `--features rest` to enable REST:

```bash
mempal serve
```

With REST enabled:

- MCP still runs over stdio
- REST listens on `127.0.0.1:3080`
- CORS only allows localhost origins

Endpoints:

- `GET /api/status`
- `GET /api/search?q=...&wing=...&room=...&top_k=...`
- `POST /api/ingest`
- `GET /api/taxonomy`

Examples:

```bash
curl 'http://127.0.0.1:3080/api/status'
curl 'http://127.0.0.1:3080/api/search?q=clerk&wing=myapp'
curl -X POST 'http://127.0.0.1:3080/api/ingest' \
  -H 'content-type: application/json' \
  -d '{"content":"decided to use Clerk","wing":"myapp","room":"auth"}'
curl 'http://127.0.0.1:3080/api/taxonomy'
```

## Benchmark LongMemEval

`mempal` includes a native LongMemEval harness. It reuses the dataset shape and retrieval metrics documented in `mempalace`, while indexing and searching through `mempal` itself.

Default session-granularity raw benchmark:

```bash
mempal bench longmemeval /path/to/longmemeval_s_cleaned.json
```

Other modes:

```bash
mempal bench longmemeval /path/to/longmemeval_s_cleaned.json --mode aaak
mempal bench longmemeval /path/to/longmemeval_s_cleaned.json --mode rooms
```

Turn granularity and results log:

```bash
mempal bench longmemeval /path/to/longmemeval_s_cleaned.json \
  --granularity turn \
  --out benchmarks/results_longmemeval.jsonl
```

Supported options:

- `--mode raw|aaak|rooms`
- `--granularity session|turn`
- `--limit N`
- `--skip N`
- `--top-k N`
- `--out path/to/results.jsonl`

What the benchmark does:

- loads the cleaned LongMemEval JSON
- builds a temporary benchmark DB per question
- indexes retrieval text using the configured embedder
- runs retrieval and reports `Recall@k` and `NDCG@k`

What it does not do:

- it does not generate final answers with an LLM
- it is not the same as the official answer-generation evaluation pipeline
- `raw` mode does not automatically mean zero API cost if your embedder backend is configured as `api`

For the current local benchmark snapshot in this repository, see [`benchmarks/longmemeval_s_summary.md`](../benchmarks/longmemeval_s_summary.md).

## Recommended: Auto-Remind After Commit

mempal works best when AI agents save decision context after every commit — not just the code diff, but *why* the change was made, what was considered, and what's left to do. This is MEMORY_PROTOCOL Rule 4 (SAVE AFTER DECISIONS).

The problem: agents forget. The solution: a Claude Code hook that reminds the agent after every `git commit`.

### Setup for Claude Code

Create `.claude/settings.json` in your project root:

```json
{
  "hooks": {
    "afterToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "if echo \"$TOOL_INPUT\" | grep -q 'git commit'; then echo 'MEMPAL REMINDER: You just committed code. Call mempal_ingest to save the decision context (what was built, why, what was considered). Rule 4: SAVE AFTER DECISIONS.'; fi"
          }
        ]
      }
    ]
  }
}
```

After this, every time the agent runs `git commit`, it sees a reminder to save the decision to mempal. The agent still decides *what* to save — the hook just ensures it doesn't forget.

### What makes a good decision record

Bad (just restating the diff):
```
Added CI workflow
```

Good (captures context a future agent needs):
```
Added CI with default + all-features matrix. Deliberately omitted rustfmt
because formatting drift exists in 2 test files — cleanup is a separate
commit. Follow-up: cargo fmt --all then add fmt check step. This completes
priority #1 from drawer_mempal_default_a295458d.
```

The difference: a future agent reading the good version knows what was omitted, why, and what to do next. The bad version tells them nothing they can't learn from `git log`.

### For other AI tools

- **Codex**: Configure in `~/.codex/instructions.md` — add "After every commit, call mempal_ingest with decision context"
- **Cursor**: Add to `.cursorrules` — same instruction
- **Any MCP client**: The MEMORY_PROTOCOL in `mempal_status` already contains Rule 4; the hook is a reinforcement for clients that sometimes skip it

## Identity File

If you use `wake-up` regularly with AI agents, you can add a user-edited identity file:

```bash
mkdir -p ~/.mempal
$EDITOR ~/.mempal/identity.txt
```

Example:

```text
Role: Rust backend engineer at Acme.
Current focus: auth rewrite, Clerk migration.
Working style: small reversible edits, verify before asserting.
```

`wake-up` can include this as part of the agent context refresh.

## FAQ

### Search results look wrong or too broad

- Pass `--wing` explicitly. Global search is convenient, but it broadens retrieval.
- Pass `--room` when you already know the subsystem.
- Inspect taxonomy with `mempal taxonomy list` and add better keywords with `mempal taxonomy edit`.
- Check which embedder backend you are using. Different embedding models shift retrieval behavior.

### Search returns irrelevant results for Chinese (or other non-English) queries

The default embedding model (MiniLM-L6-v2) is English-centric. Non-English queries produce low-quality vectors and often match the wrong drawers entirely.

**For AI agents**: MEMORY_PROTOCOL rule 3a tells agents to translate queries to English before calling `mempal_search`. This is handled automatically by agents that read the protocol.

**For CLI users**: translate your query to English manually, or use the `--wing` filter to narrow scope:

```bash
# Poor results:
mempal search "它不再是一个高级原型"

# Good results:
mempal search "no longer just an advanced prototype"
```

This is a limitation of the embedding model, not the search engine. Switching to a multilingual model (e.g., multilingual-e5, BGE-M3) would fix this at the vector level but requires re-embedding all existing drawers.

### Why did ingest store relative paths instead of absolute ones?

`mempal` stores `source_file` relative to the ingest root on purpose. This keeps citations stable if you ingest the same project through different absolute paths.

### Is `raw` benchmark mode always zero API cost?

No. `raw` only means raw retrieval text. API cost depends on the embedder backend:

- local `onnx` backend: zero external API calls
- `api` backend: embedding requests still go to the configured API

### Why is `--granularity turn` so much slower?

Because it expands one session into many more indexed items. On the current `LongMemEval s_cleaned` runs in this repository, `raw + turn` was dramatically slower than `raw + session` while not improving overall retrieval quality enough to justify being the default.

### Should I use `delete` freely because it is soft-delete?

Use it carefully anyway. `delete` is safer than hard removal, but once `mempal purge` runs, the data is permanently gone.

## Verify Changes

If you modify code or behavior in this repository, the current validation baseline is:

```bash
cargo test --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
```
