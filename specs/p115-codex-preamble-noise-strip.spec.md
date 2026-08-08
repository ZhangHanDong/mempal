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

## Acceptance Criteria

Rule: wrapper-strip  Codex runtime preamble wrappers never enter memory

Scenario: each wrapper tag block is stripped with surrounding text kept
  Test:
    Package: mempal
    Filter: test_codex_all_wrapper_tags_stripped
  Given content wrapping boilerplate in each of INSTRUCTIONS, user_instructions, environment_context, recommended_plugins, and turn_aborted tags
  When strip_codex_rollout_noise runs
  Then the wrapper block and its payload are removed
  And the text before and after the block is preserved

Scenario: multi-line wrapper blocks strip while real text survives
  Test:
    Package: mempal
    Filter: test_codex_runtime_preamble_wrappers_stripped
  Given a user_instructions block, real request text, and an environment_context block
  When strip_codex_rollout_noise runs
  Then both wrapper blocks are removed entirely
  And the real request text is preserved

Scenario: wrapper-only content strips to blank
  Test:
    Package: mempal
    Filter: test_codex_wrapper_only_message_strips_to_blank
  Given content that is exactly one recommended_plugins block
  When strip_codex_rollout_noise runs
  Then the result is blank

Scenario: wrapper tags inside code fences are preserved
  Test:
    Package: mempal
    Filter: test_codex_wrapper_inside_code_fence_preserved
  Given an environment_context tag inside a fenced code block
  When strip_codex_rollout_noise runs
  Then the fenced content is preserved verbatim

Scenario: preamble-only user messages produce no transcript line
  Test:
    Package: mempal-runtime
    Filter: ingest::normalize::tests::codex_normalize_drops_runtime_preamble_user_messages
  Given a rollout whose user messages are a user_instructions block and an environment_context block followed by real text
  When normalize_codex_jsonl runs with noise stripping
  Then only the real user text and assistant reply appear in the transcript

Rule: normalize-version  Old sources become reachable for re-normalization

Scenario: the normalize version is pinned at 3
  Test:
    Package: mempal
    Filter: test_normalize_version_bump_triggers_reindex_opportunity
  Given a drawer stored with normalize_version 1
  When reindex --stale runs
  Then the source is re-normalized
  And CURRENT_NORMALIZE_VERSION equals 3

Rule: codex-detection  Detection needs message-bearing evidence

Scenario: session_meta plus context-only records is not Codex
  Test:
    Package: mempal-runtime
    Filter: ingest::detect::tests::rejects_codex_rollout_without_message_records
  Given a JSONL file with only session_meta, turn_context, and compacted records
  When detect_format runs
  Then the format is PlainText

Scenario: current rollouts with new record types still detect
  Test:
    Package: mempal-runtime
    Filter: ingest::detect::tests::detects_current_codex_rollout_with_turn_context_and_compacted
  Given a rollout containing session_meta, turn_context, response_item, compacted, and event_msg records
  When detect_format runs
  Then the format is CodexJsonl
