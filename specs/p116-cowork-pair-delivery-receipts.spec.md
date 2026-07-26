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

- Every pair push gets a deterministic `message_id`:
  `msg_` + first 12 hex of SHA-256 over `pushed_at`, `from`, and `content`
  (null-byte separated), matching the `build_triple_id` hashing pattern.
  `InboxMessage.message_id` is `Option<String>` with serde default so
  pre-P116 inbox lines still parse and drain.
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
  file. Rotation racing a concurrent append may drop an event; accepted for
  an observability log.
- Message state is derived, not stored: join `queued`/`drained` events with
  the live inbox content — `drained` event → `drained`; queued and still
  present in the target inbox → `pending`; queued, absent from inbox, no
  drained event → `lost` (the documented drain crash window).
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

## Acceptance

- `push` returns a deterministic `message_id` and appends a `queued` receipt
  event; same inputs yield the same id.
- `drain` with receipt metadata appends one `drained` event per message with
  `injected_as`/`hook_runtime`; plain `drain` keeps working with no metadata.
- Derived states cover pending / drained / lost as specified.
- Receipts file never exceeds 400 events.
- `mempal cowork-receipts` prints the joined states in plain and json formats;
  `cowork-status` shows the summary counts.
- `mempal_cowork_push` response includes `message_id` and the MCP output
  schema requires it.
- `cargo test` (workspace), `cargo clippy`, `cargo fmt --check` pass.
