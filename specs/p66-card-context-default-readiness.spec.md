spec: task
name: "P66: card context default readiness"
inherits: project
tags: ["phase-3", "runtime-adoption", "card-context", "cli", "mcp"]
---

## Intent

P65 can summarize runtime adoption evidence, but agents still need a focused
answer to whether card-aware context is ready to be considered for default
runtime behavior. P66 adds a read-only readiness report for `include_cards` that
uses the existing card-context adoption evidence without changing defaults or
relaxing the Phase-3 evidence-first boundary.

## Decisions

- Add CLI `mempal phase3 readiness card-context-default`.
- Add MCP `mempal_phase3 action=readiness` with `candidate=card-context-default`.
- The readiness report is read-only and returns `writes=false`, `candidate`,
  `ready`, `decision`, `required_track`, `required_feature`, `review`, and
  `reasons`.
- The report reuses P65 review aggregation filtered to
  `track=card_context` and `feature=include_cards`.
- `ready=true` requires at least 3 accepted signals, zero rollback signals,
  zero contradiction signals, and accepted signals greater than or equal to
  rejected plus misses.
- `ready=true` only means eligible for a future default-on spec; it must not
  enable cards by default.
- Unsupported readiness candidates are rejected without mutation.

## Boundaries

### Allowed Changes
- `src/core/phase3.rs`
- `src/main.rs`
- `src/mcp/**`
- `src/core/protocol.rs`
- `tests/phase3_runtime.rs`
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-13-p66-card-context-default-readiness.md`
- `specs/p66-card-context-default-readiness.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change `mempal context` default `include_cards=false`.
- Do not add card embeddings.
- Do not mutate knowledge lifecycle, card lifecycle, or runtime defaults.
- Do not treat readiness as sufficient authority to implement default-on card
  context without a later explicit spec.

## Acceptance Criteria

Scenario: CLI readiness reports ready with sufficient evidence
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_ready
  Level: integration
  Given an empty CLI HOME
  And three accepted card context events exist for feature `include_cards`
  When running `mempal phase3 readiness card-context-default --format json`
  Then stdout is valid JSON
  And `writes` is false
  And `ready` is true
  And `decision` is `eligible_for_future_default_spec`
  And `review.stats.accepted` is 3
  And runtime adoption event count remains 3

Scenario: CLI readiness blocks without evidence
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_blocks_without_evidence
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 readiness card-context-default --format json`
  Then stdout is valid JSON
  And `writes` is false
  And `ready` is false
  And `decision` is `keep_opt_in`
  And `reasons` mentions insufficient accepted evidence
  And runtime adoption event count remains zero

Scenario: CLI readiness blocks rollback evidence
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_readiness_card_context_default_blocks_rollback
  Level: integration
  Given an empty CLI HOME
  And card context evidence includes accepted and rollback signals
  When running `mempal phase3 readiness card-context-default --format json`
  Then stdout is valid JSON
  And `ready` is false
  And `decision` is `keep_opt_in`
  And `reasons` mentions rollback evidence

Scenario: MCP readiness is read-only
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_readiness_card_context_default_is_read_only
  Level: unit
  Given an empty test database with card context evidence
  When `mempal_phase3` is called with `action=readiness` and `candidate=card-context-default`
  Then the response includes `writes=false`
  And the response includes a readiness report
  And runtime adoption event count remains unchanged

Scenario: Unsupported readiness candidate is rejected
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_readiness_rejects_unknown_candidate
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 readiness unknown --format json`
  Then the command fails
  And stderr mentions `unsupported phase3 readiness candidate`

## Out of Scope

- Turning `include_cards` on by default.
- Changing existing Phase-3 gate thresholds.
- New adoption event write paths.
- New schema, embeddings, or retrieval ranking changes.
