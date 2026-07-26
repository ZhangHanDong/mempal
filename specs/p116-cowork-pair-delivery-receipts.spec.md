spec: task
name: "P116: cowork pair-push delivery receipts"
inherits: project
tags: [cowork, inbox, receipts, cli, mcp, observability]
estimate: 1d
---

## Intent

P116 answers GitHub issue #81: the P8 pair channel (`mempal_cowork_push` +
hook-driven `cowork-drain`) is honest about at-next-UserPromptSubmit delivery,
but leaves no audit trail. When a partner says "I never saw that handoff",
nobody can tell whether the message was never queued, queued but never
drained, or drained and injected. P88 already gives the multi-agent bus a
delivery ack/status surface; P116 ports the receipt idea to the pair channel
while keeping its zero-DB, ephemeral, file-based design.

## Decisions

- Every pair push gets a deterministic base `message_id`:
  `msg_` + first 12 hex of SHA-256 over `pushed_at`, `from`, and `content`
  (null-byte separated), matching the `build_triple_id` hashing pattern.
  Because `pushed_at` is second-precision, `push_with_receipt` uniquifies
  the handle against ids already present in the receipts log and the live
  inboxes (`-2`, `-3`, … suffix) so same-second identical pushes never
  share a handle. `InboxMessage.message_id` is `Option<String>` with serde
  default so pre-P116 inbox lines still parse and drain.
- `mempal_cowork_push` returns the `message_id` in `CoworkPushResponse`
  (always serialized, schema-required) so the sender holds a receipt handle
  immediately.
- New append-only receipts log per project identity:
  `~/.mempal/cowork-inbox/receipts/<encoded_project_identity>.jsonl`.
  `push` appends a `queued` event; `drain` appends one `drained` event per
  drained message carrying `injected_as` (the drain output format) and an
  optional `hook_runtime` label.
- Receipt writes are best-effort observability: an IO failure appending a
  receipt must never fail or roll back the push/drain itself.
- Receipts rotate in place: keep at most the newest 400 events per project
  file. Appends are serialized with the existing P9 `ingest::lock` flock
  (Unix; Windows falls back to the documented no-op lock), so the cap holds
  under concurrent push/drain. If lock acquisition itself fails, the append
  proceeds unlocked — delivery observability beats strict rotation.
- Message state is derived, not stored: join `queued`/`drained` events with
  the live inbox content — `drained` event → `drained`; queued and still
  present in the target inbox → `pending`; queued, absent from inbox, no
  drained event → `lost` (the documented drain crash window). The join is
  multiset-safe (k-th queued pairs with k-th drained per id), and live
  inbox messages whose `queued` receipt was lost still surface as
  `pending` rows.
- `cowork-drain` validates `--format` BEFORE the destructive drain rename,
  and records `drained` receipts only after the hook output has been
  written to stdout — an invalid format must lose no messages and write no
  false receipt (exit stays 0 per the hook graceful-degrade contract).
- New read-only CLI `mempal cowork-receipts --cwd <path> [--limit N]
  [--format plain|json]` prints per-message states newest-first plus counts.
  `cowork-status` output gains a one-line receipts summary per target.
- `cowork-drain` gains optional `--hook-runtime <label>` recorded verbatim in
  drained events. Installed hook templates are NOT changed in P116 (avoids
  stale-entry churn in `cowork-install-hooks` self-heal); users may add the
  flag manually.
- Explicitly out of scope, documented here per issue #81 triage:
  runtime-side "injected but ignored" detection (unknowable in-process;
  `codex_hooks` flag issues stay covered by install-hooks warning + doctor),
  `persisted_drawer_id` receipts (persistence is the explicit P96 capture
  path), and TTL/supersede (`stale_if`) semantics (no evidence yet).

## Boundaries

### Allowed Changes
- crates/mempal-runtime/src/cowork/inbox.rs
- crates/mempal-runtime/src/cowork/receipts.rs
- crates/mempal-runtime/src/cowork/mod.rs
- crates/mempal-runtime/src/cowork/bus.rs (compile-only: fill the new
  `InboxMessage.message_id` field with `None`; no semantics change)
- crates/mempal-mcp-server/src/tools.rs
- crates/mempal-mcp-server/src/server.rs
- src/main.rs
- src/cowork/mod.rs
- tests/cowork_receipts.rs
- specs/p116-cowork-pair-delivery-receipts.spec.md
- docs/plans/2026-07-26-p116-cowork-pair-delivery-receipts.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not write receipts (or anything else) to palace.db; the pair channel
  stays file-based and ephemeral.
- Do not change the P84-P101 bus (`cowork/bus.rs`) event/delivery semantics.
- Do not change drain's at-most-once winner-takes-all rename semantics or
  the inbox size/count caps.
- Do not modify the installed hook script templates or
  `cowork-install-hooks` behavior.
- Do not break parsing of pre-P116 inbox jsonl lines.

## Acceptance Criteria

Rule: receipt-handle  Every push yields a unique, trackable receipt handle

Scenario: message_id is deterministic over its inputs
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::message_id_is_deterministic_and_prefixed
  Given the same pushed_at, from, and content
  When build_message_id runs twice
  Then both calls return the same `msg_` + 12-hex handle

Scenario: same-second identical pushes get distinct handles
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::same_second_identical_pushes_get_distinct_ids_and_both_drain
  Given two pushes with identical content, from, and second-precision pushed_at
  When both are pushed with receipts and then drained
  Then the two receipt handles differ
  And both messages report status drained

Scenario: push records a queued receipt event
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::push_returns_message_id_and_appends_queued_event
  Given an empty project inbox
  When push_with_receipt succeeds
  Then the receipts log contains one queued event carrying the returned handle

Scenario: MCP push returns the schema-required handle
  Test:
    Filter: cargo test -p mempal-mcp-server --lib test_mcp_push_returns_message_id_and_schema_requires_it
  Given an MCP client recognized as codex
  When mempal_cowork_push succeeds
  Then the response message_id starts with msg_
  And message_id is in the CoworkPushResponse schema required list

Rule: drain-receipts  Drains record how messages were injected — and only then

Scenario: drain records injected_as and hook_runtime per message
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::drain_with_receipt_appends_drained_events_with_meta
  Given one pushed message
  When drain_with_receipt runs with injected_as and hook_runtime metadata
  Then one drained event per message carries that metadata

Scenario: pre-P116 inbox lines drain with a null-handle receipt
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::legacy_inbox_line_without_message_id_still_drains_with_receipt
  Given an inbox line written before message_id existed
  When drain_with_receipt runs
  Then the message drains normally
  And its drained event has no message_id

Scenario: invalid drain format loses no messages and writes no receipt
  Test:
    Filter: cargo test --test cowork_receipts cowork_drain_invalid_format_preserves_inbox_and_writes_no_receipt
  Level: integration
  Given one pending message in the codex inbox
  When `mempal cowork-drain --format codex-hook-jsn` runs
  Then the command exits 0 with a format error on stderr
  And the inbox file still contains the message
  And the receipt state remains pending

Rule: derived-states  Message state is derived from events plus live inboxes

Scenario: states cover pending, drained, and lost
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::message_states_derives_pending_drained_and_lost
  Given queued messages that are respectively still in the inbox, drained, and vanished
  When message_states runs
  Then the three messages report pending, drained, and lost respectively

Scenario: duplicate ids join as a multiset
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::duplicate_id_events_join_as_multiset
  Given two queued and two drained events sharing one id
  When message_states runs
  Then both rows report drained

Scenario: a live message with no queued receipt still reports pending
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::inbox_message_without_queued_receipt_still_reported_pending
  Given an inbox line whose best-effort queued receipt write failed
  When message_states runs
  Then the message surfaces as pending with its inbox metadata

Rule: receipts-log  The log rotates at the cap even under concurrency

Scenario: rotation keeps the newest events under the cap
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::receipts_rotation_keeps_newest_events_under_cap
  Given more than 400 appended events
  When load_events runs
  Then exactly 400 events remain and the newest survives

Scenario: concurrent appends settle at the cap
  Test:
    Filter: cargo test -p mempal-runtime --lib cowork::receipts::tests::concurrent_appends_never_exceed_cap
  Given 8 threads appending 480 events in total
  When all appends complete
  Then the log holds exactly 400 events

Rule: cli-surface  Receipts are inspectable read-only from the CLI

Scenario: cowork-receipts tracks a message from pending to drained
  Test:
    Filter: cargo test --test cowork_receipts cowork_receipts_tracks_pending_then_drained
  Level: integration
  Given a pushed message
  When cowork-receipts runs before and after cowork-drain --hook-runtime
  Then the json output moves from pending to drained with injected_as and hook_runtime
  And the plain output mentions the drained state
