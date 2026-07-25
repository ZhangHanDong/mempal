spec: task
name: "P115: Codex runtime preamble strip + normalize version bump"
inherits: project
tags: [ingest, normalize, noise, codex, detect]
estimate: 0.5d
---

## Intent

P115 is the post-merge hardening pass for PR #7 (Codex rollout ingest, merged
as 2fe3ce8) and the fix for GitHub issue #10 (Codex runtime preamble pollutes
imported memory). Three gaps remain after the PR #7 merge:

1. Current Codex rollouts inject runtime preamble as `role=user` messages
   (`<user_instructions>` carrying AGENTS.md, `<environment_context>`,
   `<recommended_plugins>`, legacy `<INSTRUCTIONS>`), which the new
   `response_item` normalize path stores verbatim — issue #10 persists.
2. PR #7 changed normalization output (Codex `response_item` preference and the
   shared `extract_content_text` block-text extraction used by the
   Claude-export and ChatGPT paths) without bumping
   `CURRENT_NORMALIZE_VERSION`, so `reindex --stale` cannot find sources
   normalized under the old rules.
3. Detection (`is_codex_jsonl`) now accepts a file containing only
   `session_meta` plus a `turn_context`/`compacted` record — zero messages —
   and any third-party JSONL with a `session_meta`-typed line can be
   misrouted to the Codex path.

## Decisions

- Strip Codex runtime wrapper blocks in `strip_codex_rollout_noise` for the
  evidence-based tag set: `<INSTRUCTIONS>`, `<user_instructions>`,
  `<environment_context>`, `<recommended_plugins>`, `<turn_aborted>`.
  Block strip is paired-tag removal (open through matching close), code-fence
  aware, reusing the existing system-reminder strip machinery generalized to
  arbitrary marker pairs.
- A user message that consists entirely of wrapper blocks strips to empty and
  is dropped by the existing empty-message filtering; no separate
  message-level skip list.
- Bump `CURRENT_NORMALIZE_VERSION` from 2 to 3 exactly once for the combined
  PR #7 + P115 normalization change, so one `mempal reindex --stale` pass
  re-normalizes previously ingested Claude/Codex/ChatGPT sources.
- Tighten `is_codex_jsonl`: a file is Codex JSONL only if it has
  `session_meta` AND at least one message-bearing record type (`event_msg` or
  `response_item`). `turn_context`, `compacted`, and unknown typed records
  stay tolerated but are not sufficient evidence on their own.
- Do not change the legacy `event_msg` fallback ordering, chunking, drawer
  identity, schema version, or any MCP tool contract in P115.

## Boundaries

### Allowed Changes
- crates/mempal-runtime/src/ingest/noise.rs
- crates/mempal-runtime/src/ingest/normalize.rs
- crates/mempal-runtime/src/ingest/detect.rs
- tests/noise_strip.rs
- specs/p115-codex-preamble-noise-strip.spec.md
- docs/plans/2026-07-26-p115-codex-preamble-noise-strip.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not add a schema migration, database table, or database column.
- Do not rewrite, delete, or re-embed existing drawers (re-normalization only
  happens through the explicit `reindex --stale` flow).
- Do not change drawer identity hashing, search ranking, or MCP response
  shapes.
- Do not strip wrapper tags that appear inside fenced code blocks.

## Acceptance

- `strip_codex_rollout_noise` removes each listed wrapper block, including
  multi-line blocks, and preserves surrounding real text.
- A Codex rollout whose only user-message content is a wrapper block produces
  no transcript line for that message.
- `CURRENT_NORMALIZE_VERSION == 3`.
- `detect_format` returns `PlainText` for a JSONL file with `session_meta` +
  `turn_context` only, and `CodexJsonl` once an `event_msg` or `response_item`
  record is present.
- `cargo test` (workspace), `cargo clippy`, `cargo fmt --check` pass.
