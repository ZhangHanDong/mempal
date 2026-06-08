# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-1.0: MINOR bumps introduce new features, PATCH bumps are bug-fix only).

## [0.6.1] — 2026-06-09

Quality / discoverability patch. **P107: make the evidence-vs-knowledge ingest
boundary discoverable before failure.** No schema, behavior, or validation
change — only the surfaces a first-time agent reads before calling.

- `mempal_ingest` tool description now states it writes a raw **evidence**
  drawer by default and that knowledge-only fields belong to
  `mempal_knowledge_distill`, not a default ingest.
- The `IngestRequest` knowledge-only fields (`memory_kind`, `statement`,
  `tier`, `status`, `supporting_refs`, `counterexample_refs`, `teaching_refs`,
  `verification_refs`, `scope_constraints`, `trigger_hints`) carry doc comments,
  so the derived MCP JSON schema marks them knowledge-only and steers to
  distill.
- The `evidence drawer does not allow knowledge-only fields` rejection is now
  remedial: it names the fields and tells the caller to omit them or use
  `mempal_knowledge_distill`.
- The embedded MEMORY_PROTOCOL (Rule 4) documents the evidence-vs-knowledge
  entrypoint split.

Motivated by a real Codex miscall: it passed a distilled `statement` +
`supporting_refs` through `mempal_ingest` (which defaults to evidence) and hit
the rejection with no next-step guidance. A drawer storing the lesson is
pull-based; a first-time agent never searches it, so the contract had to move to
push-based surfaces (tool description, schema docs, error text, protocol).

## [0.6.0] — 2026-06-05

Feature release. **P106: a read-only "distill signal" in mind-model context.**

### Added

- **`mempal context` / `mempal_context` now carry `distill_suggestions`** — a
  read-only, deterministic signal that flags fields worth crystallizing into
  knowledge. The detector groups active drawers by `field` and surfaces a
  suggestion for each field with at least 5 active evidence drawers AND zero
  active promoted-or-canonical knowledge. It returns at most 3 suggestions
  (descending evidence count, then ascending field); each carries `field`,
  `evidence_count`, up to 3 `sample_evidence_drawer_ids`, and
  `suggested_tier="dao_ren"`. This is the "detector" layer of agent-driven
  mind-model construction: it makes "this is worth distilling" a client-agnostic,
  pull-based signal that appears where agents already look.
- On by default; disable per call with the CLI `--no-distill-suggestions` flag
  or the MCP `include_distill_suggestions=false` request field. `mempal brief`
  does not carry the signal.

### Notes

- Purely observational: the detector performs no database write, no LLM call, no
  auto-distill, and no auto-promotion. Acting on a suggestion stays the agent's
  explicit `mempal_knowledge_distill` plus the deterministic gate (governance per
  P77/P80 unchanged). It never alters the assembled tier sections.

## [0.5.4] — 2026-05-30

Bug-fix release. **`purge_deleted` could silently drop triple provenance when a
hard delete was blocked by another foreign key** (a non-atomic edge case left by
0.5.3's FK fix).

### Fixed

- **`purge_deleted` is now atomic.** 0.5.3 cleared `triples.source_drawer` before
  hard-deleting a drawer, but `purge_deleted` ran each statement outside a
  transaction. If the subsequent `DELETE FROM drawers` was blocked by another
  `RESTRICT` foreign key — e.g. `knowledge_evidence_links.evidence_drawer_id`,
  which protects a card's evidence — the `source_drawer = NULL` had already
  committed, silently dropping KG provenance for a drawer that was never purged.
  The purge loop is now wrapped in `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK`, so
  a blocked delete rolls back the NULL too. The reindex replace path was already
  transactional and unaffected. Adds a regression test that blocks a purge with
  an evidence link and asserts the triple provenance survives.

## [0.5.3] — 2026-05-29

Bug-fix release. **Reindex/purge crashed when deleting a drawer referenced by a
KG triple** (surfaced while self-healing the 0.5.2 duplicate cleanup).

### Fixed

- **Hard-deleting a drawer that a KG triple references no longer fails with
  `FOREIGN KEY constraint failed`.** `triples.source_drawer` is a `RESTRICT`
  FK to `drawers(id)` and mempal opens connections with `foreign_keys=ON`, so
  the across-rooms reindex replace (and `purge_deleted`) errored out and rolled
  back when a stale drawer was referenced by a triple. Both hard-delete paths
  now clear the dangling `source_drawer` pointer (`UPDATE triples SET
  source_drawer = NULL`) before deleting the drawer — the KG fact is kept, only
  its stale provenance link is dropped. Adds a regression test. Without this,
  `mempal reindex --stale` could not finish cleaning the 0.5.2-era duplicates.

## [0.5.2] — 2026-05-29

Bug-fix release. **`reindex` left duplicate drawers when a source re-routed to
a different room.**

### Fixed

- **`reindex --stale` / `--force` no longer leave stale drawers behind when a
  source auto-routes to a new room.** Re-ingesting a source replaced its prior
  drawers only within the freshly resolved room
  (`replace_active_source_drawers` is keyed on `(source_file, wing, room)`). If
  taxonomy routing now sent the source to a different room than its existing
  drawers occupied, the old-room drawers were never deleted and coexisted with
  the new ones as duplicates — and their stale `normalize_version` could never
  be cleared. Reindex now deletes a source's prior drawers across **all** rooms
  via the new `Database::replace_active_source_drawers_across_rooms`, gated by a
  `replace_across_rooms` ingest option (reindex-only; normal ingest keeps
  room-scoped replace). Adds regression tests covering both the across-rooms
  delete and the room-scoped contrast. After upgrading, run `mempal reindex
  --stale` once to self-heal any duplicates left by an earlier version.

## [0.5.1] — 2026-05-29

Bug-fix release. **0.5.0's MCP tool list failed to load in strict clients.**

### Fixed

- **MCP tool list now loads in Claude Code (and other strict clients).** The
  `mempal_phase3` tool's `metadata` and `report` inputs are free-form JSON
  (`serde_json::Value`), for which schemars emits a boolean `true` property
  schema. Claude Code's Zod-based validator rejects a boolean property schema
  and then refuses the **entire** tool list (`tools[..].inputSchema.properties.
  {metadata,report}: Invalid input`), so all 23 tools silently disappeared in
  0.5.0. Both fields now advertise a concrete `{"type": "object"}` schema via a
  `schema_with` helper, and a regression test asserts they never revert to a
  boolean schema. CLI behavior is unchanged.

## [0.5.0] — 2026-05-29

Large feature release covering P10–P105: the mind-model knowledge layer,
Phase-2 knowledge cards, Phase-3 runtime adoption evidence, the multi-agent
cowork bus, release/ops tooling, and the first Chinese mdBook. Schema advances
to **v9**. No breaking CLI removals; existing commands keep their semantics.

### Added

- **Mind-model knowledge layer (P12–P29).** Typed drawers with `dao_tian /
  dao_ren / shu / qi / evidence` tiers and `global / repo / worktree` anchors;
  `mempal context` / `mempal_context` runtime context assembler; knowledge
  lifecycle CLI/MCP (`distill`, `gate`, `promote`, `demote`, `publish-anchor`);
  read-only promotion policy and field-taxonomy surfaces.
- **Phase-2 knowledge cards (P30–P48).** Schema v8 `knowledge_cards` /
  `knowledge_evidence_links` / `knowledge_events`; card core API, CLI, MCP
  read + gate/promote/demote/retrieve; Stage-1 → card backfill; card-aware
  context (`--include-cards`, opt-in).
- **Phase-3 runtime adoption evidence (P49–P82).** Schema v9
  `runtime_adoption_events`; `mempal phase3` (record/list/stats/gate/review/
  analytics/readiness/default-proposal/default-control/rollback-control),
  checked records, capture helpers, opt-in instrumentation wrapper, evaluator
  advisory API, research validate/ingest planning.
- **Cognitive brief (P83, P102).** `mempal brief` / `mempal_brief` —
  deterministic citation-first brief; no LLM, no DB writes.
- **Multi-agent cowork bus (P84–P96, P101).** Concrete `agent_id` registry +
  per-agent inbox, events log, delivery ack/status, presence/heartbeat,
  threads/channels, tmux transport + live peek, sessions, handoff summary, and
  explicit handoff-to-evidence capture; `mempal_cowork_bus` MCP surface.
- **Release & ops tooling (P97–P104).** `mempal doctor` / `mempal_doctor`,
  `mempal release-readiness`, `mempal maintenance guided-run`, maintenance &
  cowork runbooks, adoption analytics.
- **Chinese mdBook (P105).** `books/zh-CN` — preface + 10 chapters + appendix,
  Mermaid via committed local JS assets. Excluded from the published crate.

### Changed

- Storage schema advances to **v9**; `reindex --stale` migrates drawers behind
  the current `normalize_version`.
- `mempal_search` results carry AAAK-derived structured signals (`entities`,
  `topics`, `flags`, `emotions`, `importance_stars`); `content` stays raw.
- Published crate now also excludes `books/**` (mdBook manuscript + local
  Mermaid asset) alongside the existing `specs/**` and `docs/plans/**`.

### Notes

- Governance boundaries are deliberate: no silent promotion, evaluator stays
  advisory, research cannot define `dao` directly, and cowork runtime logs do
  not enter durable memory without explicit capture.

## [0.4.0] — 2026-04-20

First release with **write-safety** + **content-sanity** guarantees for the
Claude Code ↔ Codex cowork pair. Closes P9 (`specs/p9-fact-checker.spec.md` and
`specs/p9-ingest-lock.spec.md`).

### Added

- **`mempal_fact_check` MCP tool** (10th tool) and `mempal fact-check` CLI
  subcommand. Offline contradiction detection against the KG `triples` table
  and the AAAK entity registry. Flags three issue kinds:
  - `SimilarNameConflict` — mentioned name is ≤2 edit-distance from a known
    entity and not identical (typo / confusable).
  - `RelationContradiction` — text asserts a predicate that's in the
    incompatibility dictionary versus an existing KG triple with the same
    `(subject, object)` endpoints.
  - `StaleFact` — text asserts a triple whose KG row has `valid_to <
    now_unix_secs`.
  Pure read, zero LLM, zero network, deterministic.
- **Protocol Rule 11 "VERIFY BEFORE INGEST"** embedded in
  `mempal_status.memory_protocol`. Guides agents to call `mempal_fact_check`
  before ingesting decisions that assert entity relationships.
- **Per-source ingest lock** (advisory `flock` on Unix). Eliminates the
  TOCTOU race between concurrent `mempal_ingest` calls targeting the same
  source (Claude Code + Codex writing the same drawer simultaneously). Lock
  file lives at `~/.mempal/locks/<16-hex>.lock`; guard releases on drop.
- **`IngestStats.lock_wait_ms` / `IngestResponse.lock_wait_ms`** — optional
  field reporting how long the ingest call waited for the per-source lock.
  Non-zero values indicate observed concurrency with a peer agent. Omitted
  in dry-run and when the write path was bypassed.
- `IngestError::Lock` variant wrapping `ingest::lock::LockError`
  (`Timeout { path, timeout }` / `Io { path, source }` / `InvalidSourceKey`).

### Changed

- `MEMORY_PROTOCOL` tool list grew 9 → 10 entries; rule count 10 → 11.
- `src/aaak/mod.rs`: widened `codec` from `mod codec` to
  `pub(crate) mod codec` so the `factcheck` module can reuse
  `extract_entities` without duplicating logic. No external API change.

### Fixed

- Concurrent same-source ingest no longer produces duplicate drawers or
  mismatched `drawer_vectors` rows. Verified by the cross-thread
  `test_concurrent_ingest_same_source_single_drawer` integration test.

### Platform notes

- Linux and macOS have full lock enforcement via `flock(LOCK_EX | LOCK_NB)`
  implemented with inline `extern "C"` (no `libc` crate dependency).
- Windows currently runs a no-op fallback for the lock path — concurrent
  ingest on Windows is **not** race-protected in 0.4.0. Follow-up work will
  adopt `LockFileEx`.

### Compatibility

- Schema version unchanged (still `4`). Existing `~/.mempal/palace.db` files
  open without migration.
- No new runtime or dev-dependency in `Cargo.toml`.
- `mempal_ingest` response adds `lock_wait_ms` with
  `#[serde(skip_serializing_if = "Option::is_none")]`, so existing JSON
  consumers that ignore unknown fields see no change. Consumers that
  destructure the struct need to accept the new field.

### Internal

- New modules: `src/factcheck/{mod,names,relations,contradictions}.rs`,
  `src/ingest/lock.rs`.
- Tests added: 24 unit tests (18 factcheck + 6 ingest lock) and 18
  integration tests (10 `tests/fact_check.rs` + 8 `tests/ingest_lock.rs`),
  including a cross-thread concurrent-ingest race gate.
- Project spec index (`CLAUDE.md`) promoted `p9-fact-checker.spec.md` and
  `p9-ingest-lock.spec.md` to "completed" and registered five new draft
  specs (P10 explicit tunnels, P10 normalize_version, P11 diary daily
  rollup, P11 chunk neighbors, P11 transcript noise strip).

---

## [0.3.1] — 2026-04-16

### Fixed

- `mempal_cowork_push` now recognizes `codex-mcp-client` as a valid Codex
  MCP client identity (the actual string Codex sends per
  `codex-rs/codex-mcp/src/mcp_connection_manager.rs`). Previously, pushes
  from Codex were rejected with "cannot infer caller tool" even when
  Codex was correctly connected.

---

## [0.3.0] — 2026-04-14

First release shipping the full **Claude ↔ Codex cowork** stack (P6 + P7 +
P8) on top of hybrid search and the knowledge graph.

### Added

- **P6 — `mempal_peek_partner` MCP tool**: read the partner agent's live
  session log (Claude `.jsonl` transcripts, Codex rollout files) in place,
  without ingesting or mutating anything. Use for "what is the other agent
  doing right now" across Claude Code and Codex.
- **P6 — Memory Protocol Rules 8 & 9**: "PARTNER AWARENESS" and
  "DECISION CAPTURE" guidance embedded in `mempal_status`.
- **P7 — Structured AAAK-derived signals in search results**: every
  `mempal_search` hit now carries `entities`, `topics`, `flags`,
  `emotions`, `importance_stars` alongside raw `content`. Agents can
  filter by `DECISION` / `TECHNICAL` flags and rank by stars without
  parsing AAAK text.
- **P8 — `mempal_cowork_push` MCP tool**: send a short ephemeral handoff
  (≤ 8 KB, up to 16 pending / 32 KB per inbox) to the partner agent's
  inbox. Delivery is at-next-UserPromptSubmit, not real-time.
- **P8 — CLI commands**:
  - `mempal cowork-drain --target <claude|codex>` — drain inbox from a
    hook; exits 0 on any failure (graceful degrade).
  - `mempal cowork-status --cwd <PATH>` — read-only inbox inspection.
  - `mempal cowork-install-hooks [--global-codex]` — one-shot installer
    for the symmetric UserPromptSubmit hook on both Claude Code and
    Codex, idempotent and self-healing.
- **P8 — Memory Protocol Rule 10 "COWORK PUSH"**.
- Crate exclude list for `cargo package` — `.claude/**`, `.mcp.json`,
  `AGENTS.md`, `CLAUDE.md`, `hooks/**`, `specs/**`, `docs/plans/**` now
  stay out of the published tarball.

### Known limitations (see README)

- Codex `codex_hooks` feature flag must be enabled (`codex features
  enable codex_hooks`); `install-hooks` detects and warns.
- Codex TUI caches config at startup; restart after enabling the flag or
  re-running `install-hooks`.
- Claude Code spawns the mempal MCP server at client startup — restart
  Claude Code after upgrading the mempal binary so newly added tools
  (e.g. `mempal_cowork_push`, `mempal_fact_check` in 0.4.0) are visible.
- `mempal_cowork_push` requires the MCP client to identify as Claude or
  Codex via `ClientInfo.name` (by design for the Claude ↔ Codex pair).

---

## Earlier versions

Earlier releases (0.1.x, 0.2.x) are tracked only in Git history. Run
`git log --oneline` on the repository to inspect them.

[0.6.0]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.6.0
[0.5.4]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.5.4
[0.5.3]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.5.3
[0.5.2]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.5.2
[0.5.1]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.5.1
[0.5.0]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.5.0
[0.4.0]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.4.0
[0.3.1]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.3.1
[0.3.0]: https://github.com/ZhangHanDong/mempal/releases/tag/v0.3.0
