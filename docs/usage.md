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
backend = "model2vec"
# model = "minishlab/potion-multilingual-128M" # default multilingual model
```

Use local ONNX instead of the default model2vec backend:

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "onnx"
```

Use an external embedding API instead of local embeddings:

```toml
db_path = "~/.mempal/palace.db"

[embed]
backend = "api"
api_endpoint = "http://localhost:11434/api/embeddings"
api_model = "nomic-embed-text"
```

Notes:

- `model2vec` is the default backend.
- The default local model is `minishlab/potion-multilingual-128M`.
- First use of `model2vec` or `onnx` may download model assets.
- If `config.toml` is missing, `mempal` still works with defaults.
- The benchmark and search commands use whatever embedder backend is configured here.

## Command Cheat Sheet

Use this when you already know the concepts and just need the right command quickly.

| Command | Purpose |
|---------|---------|
| `mempal init <DIR> [--dry-run]` | infer a `wing` and seed initial taxonomy rooms from a project tree |
| `mempal ingest --wing <WING> <DIR> [--dry-run]` | chunk, embed, and store a project tree |
| `mempal search <QUERY> [--wing W] [--room R] [--json]` | hybrid search (BM25 + vector + RRF) with tunnel hints |
| `mempal context <QUERY> [--format json] [--include-evidence] [--dao-tian-limit N]` | assemble mind-model runtime context (`dao_tian -> dao_ren -> shu -> qi`); default `dao_tian` budget is 1 |
| `mempal field-taxonomy [--format json]` | inspect read-only recommended `field` values for typed memory |
| `mempal knowledge distill --statement ... --content ... --tier dao_ren --supporting-ref <ID>` | create candidate knowledge from evidence refs |
| `mempal knowledge policy [--format json]` | inspect read-only Stage-1 promotion policy thresholds |
| `mempal knowledge gate <ID> [--format json]` | evaluate whether knowledge satisfies promotion gate policy without mutating it |
| `mempal knowledge promote <ID> --status promoted --verification-ref <ID> --reason ...` | promote bootstrap knowledge into active runtime use |
| `mempal knowledge demote <ID> --status demoted --evidence-ref <ID> --reason ... --reason-type contradicted` | demote or retire contradicted / obsolete bootstrap knowledge |
| `mempal wake-up [--format aaak]` | L0/L1 refresh sorted by importance; not a typed mind-model context pack |
| `mempal compress <TEXT>` | format arbitrary text as AAAK |
| `mempal kg add <S> <P> <O> [--source-drawer ID]` | add a knowledge graph triple |
| `mempal kg query [--subject S] [--predicate P] [--object O]` | query triples |
| `mempal kg timeline <ENTITY>` | chronological view of an entity's relationships |
| `mempal kg stats` | knowledge graph statistics |
| `mempal tunnels` | discover rooms shared across multiple wings |
| `mempal taxonomy list` | inspect current routing keywords |
| `mempal taxonomy edit <WING> <ROOM> --keywords ...` | tune routing behavior |
| `mempal reindex` | re-embed all drawers after model/backend change |
| `mempal status` | schema version, drawer counts, triples, deleted drawers, scopes |
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

### Bootstrap Knowledge Lifecycle

P18 adds the explicit Stage-1 distillation entry point: create candidate knowledge
from existing evidence drawers.

```bash
mempal knowledge distill \
  --statement "Prefer evidence before asserting project facts" \
  --content "When answering project-specific questions, cite source-backed memory before making claims." \
  --tier dao_ren \
  --supporting-ref drawer_evidence
```

Distill always creates `status=candidate` and currently only allows `tier=dao_ren`
or `tier=qi`. `dao_tian` and `shu` are intentionally excluded from candidate
distill because the current P12 status policy does not allow candidate states
for those tiers. Use `promote` only after review.

P17 adds manual lifecycle commands for Stage-1 knowledge drawers. P19 hardens
those commands so lifecycle refs must be existing evidence drawers, not arbitrary
ids or other knowledge drawers:

P18 adds deterministic CLI distill. P22 exposes the same operation to MCP agents
as `mempal_knowledge_distill`: create candidate `dao_ren` / `qi` knowledge from
existing evidence refs without LLM summarization or auto-promotion. P23 exposes
the lifecycle mutation side as `mempal_knowledge_promote` and
`mempal_knowledge_demote`: MCP promotion is gate-enforced, and demotion requires
counterexample evidence.

Equivalent MCP distill request:

```json
{
  "statement": "Prefer evidence first",
  "content": "Use cited evidence before asserting project facts.",
  "tier": "dao_ren",
  "supporting_refs": ["drawer_evidence"]
}
```

P20 adds a read-only promotion gate report. P27 exposes the current Stage-1
policy table directly:

```bash
mempal knowledge policy --format json
```

Use `gate` before `promote` to check the minimum deterministic policy against a
specific drawer without changing status, refs, vectors, schema, or the audit
log. P21 exposes the same drawer-specific gate to MCP agents as
`mempal_knowledge_gate`, while P27 exposes the policy table as
`mempal_knowledge_policy`.

```bash
mempal knowledge gate drawer_knowledge --format json
```

P24 adds explicit anchor publication. This is separate from tier/status
promotion: it only moves an already active knowledge drawer outward across
anchor scope, without rewriting content or vectors.

```bash
mempal knowledge publish-anchor drawer_knowledge \
  --to repo \
  --reason "stable across this repository"
```

Supported publication chain is `worktree -> repo -> global`. Publishing to
`global` requires `domain=global` and an explicit `--target-anchor-id
global://...`. P25 exposes the same metadata-only operation to MCP agents as
`mempal_knowledge_publish_anchor`.

For `dao_tian -> canonical`, provide a reviewer for the advisory gate:

```bash
mempal knowledge gate drawer_dao_tian \
  --target-status canonical \
  --reviewer human \
  --format json
```

Equivalent MCP request:

```json
{
  "drawer_id": "drawer_dao_tian",
  "target_status": "canonical",
  "reviewer": "human"
}
```

```bash
mempal knowledge promote drawer_knowledge \
  --status promoted \
  --verification-ref drawer_evidence \
  --reason "validated across repeated runs" \
  --reviewer "human"
```

```bash
mempal knowledge demote drawer_knowledge \
  --status demoted \
  --evidence-ref drawer_counterexample \
  --reason "new evidence contradicts this heuristic" \
  --reason-type contradicted
```

Lifecycle commands only update existing `memory_kind=knowledge` drawers. They validate that `--verification-ref` / `--evidence-ref` values start with `drawer_`, exist, and point to `memory_kind=evidence`. They do not change content, re-embed vectors, bump schema, or add Phase-2 `knowledge_cards`. Successful distill and lifecycle changes append JSONL audit entries.

Phase-2 knowledge cards are not implemented yet. The design target is a future
same SQLite palace.db table split: `drawers` remain the evidence/citation root,
while `knowledge_cards`, `knowledge_evidence_links`, and `knowledge_events`
become separate tables in the same database.

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

Use `mempal context` when the agent needs typed operating guidance such as
`dao_tian -> dao_ren -> shu -> qi`. `wake-up` may show selected knowledge
statements, but it keeps the L0/L1 refresh shape and does not assemble typed
tier sections or apply `dao_tian_limit`.

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

### Field Taxonomy

`field` is a mind-model metadata dimension used by typed memory search and
context assembly. It is separate from wing/room routing taxonomy. P28 exposes a
read-only recommended field list:

```bash
mempal field-taxonomy
mempal field-taxonomy --format json
```

The field taxonomy is guidance only. Custom fields remain valid for ingest,
distill, search, and context when the recommended Stage-1 fields are too coarse.

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

This section is about AAAK output formatting, not retrieval quality. Chinese AAAK support is currently stronger than Chinese search quality.

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

### Agent Diary

mempal supports cross-session behavioral learning through a diary convention. Agents (or humans) record observations, lessons, and patterns that future sessions can learn from.

The diary uses existing tools — no special commands needed:

```bash
# Write a diary entry (via MCP or by asking your AI agent)
# Convention: wing="agent-diary", room=agent-name
# Prefix content with OBSERVATION:, LESSON:, or PATTERN:

# Search diary entries
mempal search "lesson" --wing agent-diary
mempal search "pattern infrastructure" --wing agent-diary --room claude

# Search all entries for a specific agent
mempal search "observation" --wing agent-diary --room codex
```

Example diary entry (written by AI agent via MCP `mempal_ingest`):

```json
{
  "content": "LESSON: always check repo docs before writing infrastructure code",
  "wing": "agent-diary",
  "room": "claude",
  "importance": 4
}
```

Content prefixes:

| Prefix | Use for |
|--------|---------|
| `OBSERVATION:` | Factual behavioral observations |
| `LESSON:` | Actionable takeaways from mistakes or successes |
| `PATTERN:` | Recurring behavioral patterns across sessions |

MEMORY_PROTOCOL Rule 5a tells AI agents to write diary entries after sessions. Human users can also write diary entries — use `room` to identify the author (e.g., `room="alex"`).

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

The MCP server exposes eighteen tools:

- `mempal_status` — state + protocol + AAAK spec
- `mempal_search` — hybrid search (BM25 + vector + RRF) with tunnel hints and AAAK-derived structured signals (`entities` / `topics` / `flags` / `emotions` / `importance_stars`)
- `mempal_context` — mind-model runtime context pack (`dao_tian -> dao_ren -> shu -> qi`, evidence opt-in, `dao_tian_limit` default 1); guides workflow / skill / tool choice but never executes skills
- `mempal_field_taxonomy` — read-only recommended `field` values for typed evidence / knowledge; guidance only, custom fields remain valid
- `mempal_knowledge_distill` — create candidate `dao_ren` / `qi` knowledge from existing evidence refs; deterministic and never auto-promotes
- `mempal_knowledge_policy` — read-only Stage-1 promotion policy table for `dao_tian`, `dao_ren`, `shu`, and `qi`
- `mempal_knowledge_gate` — read-only promotion readiness check for knowledge drawers; returns the same deterministic gate report as `mempal knowledge gate --format json`
- `mempal_knowledge_promote` — gate-enforced lifecycle promotion with supplied verification refs
- `mempal_knowledge_demote` — demote or retire knowledge with counterexample evidence refs
- `mempal_knowledge_publish_anchor` — metadata-only outward anchor publication for active knowledge
- `mempal_ingest` — store memories with optional importance (0-5) and dry_run
- `mempal_delete` — soft-delete with audit
- `mempal_taxonomy` — list or edit routing keywords
- `mempal_kg` — knowledge graph: add/query/invalidate/timeline/stats
- `mempal_tunnels` — cross-wing room discovery
- `mempal_peek_partner` — read the partner coding agent's live session (Claude ↔ Codex); pure read, never writes to mempal
- `mempal_cowork_push` — send a short handoff message (≤ 8 KB) to the partner agent's inbox; delivered at the partner's next UserPromptSubmit via a drain hook
- `mempal_fact_check` — offline contradiction detection against KG triples and known entities

The server also embeds MEMORY_PROTOCOL (behavioral rules) in the MCP `initialize.instructions` field so any MCP client learns the workflow on connect — zero configuration. The protocol treats `wake-up` as an L0/L1 refresh surface, `mempal_context` as typed guidance for choosing an approach, workflow, skill, or tool, `mempal_field_taxonomy` as guidance for choosing typed-memory `field` values, and `trigger_hints` as bias metadata only. These hints never override system, user, repo, or client-native skill rules.

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

## Agent Cowork (peek + push)

Two coding agents running on the same repo — typically Claude Code and Codex — can collaborate through two primitives:

- **`mempal_peek_partner`** (P6) — read the partner's live session file without storing anything in mempal. Use for "what is partner currently doing" questions.
- **`mempal_cowork_push`** (P8) — send a short handoff (≤ 8 KB) to the partner's inbox. The partner sees it prepended to their next user prompt via a UserPromptSubmit hook. Use for "make sure partner notices X" status updates that are too transient for an ingest drawer.

### Install hooks

Hooks land the pushed message into the partner's next prompt automatically. Install once per repo (run at the repo root):

```bash
mempal cowork-install-hooks --global-codex
```

This writes two Claude-side artifacts (both required — Claude Code does not auto-discover bare hook scripts):

- `.claude/hooks/user-prompt-submit.sh` — the drain script
- an entry in `.claude/settings.json` under `hooks.UserPromptSubmit` registering the script

and merges the equivalent entry into `~/.codex/hooks.json` (top-level `hooks.UserPromptSubmit` with `{type:"command", command:"mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json"}`).

Re-running is idempotent and self-heals stale/wrong drain entries from prior mempal versions, preserving any unrelated hooks already in those files.

### Check current state

```bash
mempal cowork-status --cwd "$PWD"
```

Lists both inbox targets (`claude` and `codex`) for the given cwd along with message counts, byte sizes, and a preview. Read-only — does not drain.

### Known limitations

- **Codex `codex_hooks` feature flag**: Codex's hooks runtime is gated behind the `codex_hooks` feature flag ("under development" in current shipped `codex-cli`). If the flag is off, Codex silently ignores `~/.codex/hooks.json`. `install-hooks` detects this and prints an activation prompt (`codex features enable codex_hooks`).
- **TUI restart required on Codex side**: Codex caches `config.toml` + `hooks.json` at TUI startup only. After changing the feature flag or running `install-hooks`, fully quit and relaunch Codex before new hooks take effect.
- **MCP server re-spawn required in Claude Code**: Claude Code spawns the mempal MCP server at client startup. After upgrading the mempal binary, restart Claude Code so the MCP server respawns with the new tool list (notably `mempal_cowork_push`).
- **Claude ↔ Codex scope**: `mempal_cowork_push` requires the MCP client to identify itself as `claude-code` or `codex` (or their recognized aliases). Generic MCP clients cannot push because caller identity is required to fill the message `from` field and enforce self-push rejection. This is by design for the Claude ↔ Codex pair.
- **At-next-submit, not real-time**: a push is visible on the partner's *next* user prompt turn — never mid-turn. Codex's TUI will not re-render to inject a message on an external trigger.

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

For the current local benchmark snapshot in this repository, see [`benchmarks/longmemeval_s_summary.md`](../benchmarks/longmemeval_s_summary.md). That summary now separates the older 384d baseline from the newer model2vec 256d run.

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

## Auto-Dream Integration

Claude Code's auto-dream feature consolidates session memory while you're away — like REM sleep for AI. mempal integrates with this process to ensure project decisions survive across sessions.

### How it works

When auto-dream runs (automatically between sessions or manually via "dream"):

1. Claude reviews recent session transcripts
2. Extracts key decisions and knowledge
3. **With mempal**: verifies facts via `mempal_search`, saves consolidated insights via `mempal_ingest` with importance >= 3, and records a dream diary entry

### Setup

Add to your project's `CLAUDE.md`:

```markdown
## Auto-Dream Integration

When performing auto-dream or manual dream:
1. Call mempal_search to verify facts being consolidated
2. Save high-value insights to mempal (mempal_ingest, importance >= 3)
3. If MEMORY.md and mempal contradict, trust mempal (has citations)
4. Write dream summary as agent diary (wing="agent-diary", room="claude")
5. Check triples for expired relationships to invalidate
```

### What this gives you

Without mempal, auto-dream consolidates into MEMORY.md files that only Claude Code reads. With mempal, dream insights are stored in `palace.db` where **any** MCP-connected agent (Codex, Cursor, etc.) can find them. Dream becomes a cross-agent memory consolidation mechanism, not just a Claude Code internal process.

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

The default embedder is now a multilingual `model2vec` model, but English queries still retrieve more reliably than Chinese (and other non-English) queries in practice.

**For AI agents**: MEMORY_PROTOCOL rule 3a tells agents to translate queries to English before calling `mempal_search`. This is handled automatically by agents that read the protocol.

**For CLI users**: translate your query to English manually, or use the `--wing` filter to narrow scope:

```bash
# Poor results:
mempal search "它不再是一个高级原型"

# Good results:
mempal search "no longer just an advanced prototype"
```

This is mostly a retrieval-stack limitation, not a storage limitation:

- the embedder is multilingual but still stronger on English queries
- the search path does not currently use a Chinese-specific lexical tokenizer for FTS5
- AAAK uses `jieba-rs`, but the search path does not

So the practical guidance is still: translate the query to English first, or narrow scope with `--wing` / `--room`.

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
