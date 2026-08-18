spec: task
name: "P117: atomic reindex replace + dry-run feasibility report"
inherits: project
tags: [ingest, reindex, store, sqlite, data-safety]
estimate: 0.5d
---

## Intent

P117 removes every known data-loss window in source replacement and makes
`reindex --stale --dry-run` an honest feasibility report. Replacement must
reject incomplete embedding batches and insert collisions before committing,
and sources protected by Stage-1 or Phase-2 knowledge references must remain
untouched. Dry-run must distinguish reindexable, missing, and governance-
protected sources before the operator runs the P115 normalize-v3 reindex on
the production palace.db.

## Decisions

- Reorder the replace path: chunk, dedup, and embed BEFORE any destructive
  write. Embedding failure must leave the database untouched.
- Require exactly one embedding vector per pending chunk before entering the
  replacement transaction. Both short and overlong successful embedder
  responses are errors and leave the database untouched.
- The destructive window itself becomes one SQLite IMMEDIATE transaction:
  delete-old-source-drawers plus insert-new-drawers-and-vectors commit or
  roll back together, via a new `Database::with_immediate_transaction`
  helper and non-transactional `_in_txn` variants of the source-replace
  deletes (the existing public delete methods keep their own transaction
  for other callers).
- `Database::with_immediate_transaction` performs a best-effort rollback when
  either the closure or `COMMIT` fails, so the connection returns to autocommit
  whenever SQLite permits rollback.
- Under `replace_existing_source`, the per-chunk `drawer_exists` DB check
  is skipped because the source's old rows are deleted in the transaction.
  In-run duplicate detection via the seen-id set stays, and an ignored insert
  is a replacement collision error that rolls back instead of silently
  counting the missing row as skipped.
- Before classifying an on-disk source as reindexable, query Stage-1 drawer
  reference arrays and Phase-2 `knowledge_evidence_links`. Any source with at
  least one such reference is governance-protected: dry-run and real-run both
  skip it, preserve every old drawer, and expose protected source/drawer/ref
  counts in `ReindexReport` and CLI output. P117 does not guess an old-to-new
  evidence mapping.
- Repeat the knowledge-reference check inside the SQLite IMMEDIATE replacement
  transaction, and route both public source-replace methods through their
  `_in_txn` variants. This protects direct replace callers and closes the gap
  between the reporting preflight and the destructive write.
- Dry-run performs the same source-file existence scan as a real run and
  fills missing-source and governance-protected counters before returning.
- Sources without an on-disk file (e.g. MCP-ingested content) remain
  non-reindexable by design; P117 only makes that visible, it does not try
  to re-normalize them.
- No schema migration, no drawer-id change, no CLI flag or MCP contract changes;
  the reindex report and existing CLI text output gain additive safety fields.

## Boundaries

### Allowed Changes
- crates/mempal-store-sqlite/src/lib.rs
- crates/mempal-runtime/src/ingest/mod.rs
- crates/mempal-runtime/src/ingest/reindex.rs
- src/main.rs
- tests/reindex_safety.rs
- specs/p117-reindex-atomic-replace.spec.md
- docs/plans/2026-08-18-p117-reindex-atomic-replace.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add a schema migration, table, or column.
- Do not change drawer identity hashing or chunking.
- Do not change CLI flags or MCP tool contracts.
- Do not auto-delete or rewrite drawers whose source file is missing.

## Acceptance Criteria

Rule: no-loss-replace  Replacement never destroys data it cannot rebuild

Scenario: embedding failure leaves existing drawers intact
  Test:
    Package: mempal
    Filter: reindex_embed_failure_preserves_existing_drawers
  Level: integration
  Given a stale drawer whose source file exists on disk
  When reindex runs with an embedder that always fails
  Then the reindex reports an error
  And the original drawer and its content are still present and active

Scenario: incomplete embedding batch leaves existing drawers intact
  Test:
    Package: mempal
    Filter: reindex_embedding_count_mismatch_preserves_existing_drawers
  Level: integration
  Given a stale drawer whose source file exists on disk
  When reindex receives successful embedding responses with fewer and more vectors than chunks
  Then the reindex reports the expected and actual vector counts
  And the original drawer and its content are still present and active

Scenario: transaction helper rolls back the whole batch on error
  Test:
    Package: mempal
    Filter: with_immediate_transaction_rolls_back_on_error
  Level: integration
  Given an active drawer
  When a transaction deletes it and then returns an error
  Then the drawer is still present after the call

Scenario: transaction helper rolls back after commit failure
  Test:
    Package: mempal
    Filter: with_immediate_transaction_rolls_back_on_commit_failure
  Level: integration
  Given a deferred foreign-key violation created inside an IMMEDIATE transaction
  When SQLite rejects COMMIT
  Then `Database::with_immediate_transaction` returns an error
  And the connection is in autocommit with no violating row

Scenario: replacement insert collision rolls back old-source deletion
  Test:
    Package: mempal
    Filter: reindex_insert_collision_preserves_existing_drawers
  Level: integration
  Given `replace_existing_source` skips `drawer_exists` for a stale source
  And another source already owns the replacement id
  When reindex attempts to insert the colliding replacement drawer
  Then the reindex reports a collision error
  And both pre-existing drawers remain active

Scenario: reindex replaces a source without loss or duplicates
  Test:
    Package: mempal
    Filter: reindex_replaces_source_without_duplicates
  Level: integration
  Given a stale drawer whose source file exists on disk
  When reindex runs with a working embedder
  Then the source's drawers are fresh, at the current normalize version
  And no duplicate or leftover drawer remains for that source

Rule: honest-dry-run  Dry-run is a feasibility report, not a candidate count

Scenario: dry-run reports missing sources without writing
  Test:
    Package: mempal
    Filter: reindex_dry_run_reports_missing_sources
  Level: integration
  Given one stale drawer with an existing source file and one whose source path does not exist
  When reindex runs with dry_run
  Then the report counts both candidates
  And it counts the missing source and its drawers as skipped
  And no drawer is modified

Scenario: governance references protect source replacement
  Test:
    Package: mempal
    Filter: reindex_skips_sources_with_knowledge_references
  Level: integration
  Given a stale on-disk source referenced by both a Stage-1 knowledge drawer and a Phase-2 card
  When dry-run and real-run reindex inspect the source
  Then both reports count the source, drawer, and two protecting references as skipped
  And the original evidence drawer and both knowledge references remain intact

Scenario: replacement transaction rechecks governance references
  Test:
    Package: mempal
    Filter: replace_transaction_rechecks_knowledge_references
  Level: integration
  Given a stale on-disk source protected by a Stage-1 knowledge reference
  When the direct replace ingest entry point bypasses the reindex reporting preflight
  Then the SQLite replacement transaction reports the protected source
  And the original evidence drawer and knowledge reference remain intact

Scenario: CLI reports governance-protected sources
  Test:
    Package: mempal
    Filter: reindex_cli_reports_governance_protected_sources
  Level: integration
  Given a stale on-disk source protected by a Stage-1 knowledge reference
  When the CLI runs reindex stale in dry-run and real-run modes
  Then both stdout summaries report the protected source, drawer, and reference counts
  And no drawer is modified
