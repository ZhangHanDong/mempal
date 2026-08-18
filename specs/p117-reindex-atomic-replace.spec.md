spec: task
name: "P117: atomic reindex replace + dry-run feasibility report"
inherits: project
tags: [ingest, reindex, store, sqlite, data-safety]
estimate: 0.5d
---

## Intent

P117 removes the data-loss window in source replacement and makes
`reindex --stale --dry-run` an honest feasibility report. Today
`ingest_file_with_options` hard-deletes a source's drawers BEFORE
embeddings are generated; an embedding or insert failure afterwards leaves
the source permanently gone. And dry-run returns before the
missing-source-file scan, so a run that would actually skip most candidate
drawers (sources ingested without an on-disk file) reports them all as
reprocessable. Both must be fixed before the operator runs the P115
normalize-v3 `reindex --stale` on the production palace.db.

## Decisions

- Reorder the replace path: chunk, dedup, and embed BEFORE any destructive
  write. Embedding failure must leave the database untouched.
- The destructive window itself becomes one SQLite IMMEDIATE transaction:
  delete-old-source-drawers plus insert-new-drawers-and-vectors commit or
  roll back together, via a new `Database::with_immediate_transaction`
  helper and non-transactional `_in_txn` variants of the source-replace
  deletes (the existing public delete methods keep their own transaction
  for other callers).
- Under `replace_existing_source`, the per-chunk `drawer_exists` DB check
  is skipped: drawer ids are source-aware (P110), so a colliding id can
  only belong to this same source, whose rows are deleted in the same
  transaction. In-run duplicate detection via the seen-id set stays.
- Dry-run performs the same source-file existence scan as a real run and
  fills `skipped_missing_sources` / `skipped_missing_drawers` before
  returning; the CLI already prints these fields. Real-run behavior is
  unchanged apart from the scan happening up front.
- Sources without an on-disk file (e.g. MCP-ingested content) remain
  non-reindexable by design; P117 only makes that visible, it does not try
  to re-normalize them.
- No schema migration, no drawer-id change, no CLI flag changes.

## Boundaries

### Allowed Changes
- crates/mempal-store-sqlite/src/lib.rs
- crates/mempal-runtime/src/ingest/mod.rs
- crates/mempal-runtime/src/ingest/reindex.rs
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
  Given a stale drawer whose source file exists on disk
  When reindex runs with an embedder that always fails
  Then the reindex reports an error
  And the original drawer and its content are still present and active

Scenario: transaction helper rolls back the whole batch on error
  Test:
    Package: mempal
    Filter: with_immediate_transaction_rolls_back_on_error
  Given an active drawer
  When a transaction deletes it and then returns an error
  Then the drawer is still present after the call

Scenario: reindex replaces a source without loss or duplicates
  Test:
    Package: mempal
    Filter: reindex_replaces_source_without_duplicates
  Given a stale drawer whose source file exists on disk
  When reindex runs with a working embedder
  Then the source's drawers are fresh, at the current normalize version
  And no duplicate or leftover drawer remains for that source

Rule: honest-dry-run  Dry-run is a feasibility report, not a candidate count

Scenario: dry-run reports missing sources without writing
  Test:
    Package: mempal
    Filter: reindex_dry_run_reports_missing_sources
  Given one stale drawer with an existing source file and one whose source path does not exist
  When reindex runs with dry_run
  Then the report counts both candidates
  And it counts the missing source and its drawers as skipped
  And no drawer is modified
