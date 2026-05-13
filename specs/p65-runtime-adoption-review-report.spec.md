spec: task
name: "P65: runtime adoption review report"
inherits: project
tags: ["phase-3", "runtime-adoption", "review", "cli", "mcp"]
---

## Intent

P54-P64 can collect and preflight runtime adoption events, but agents still need
a compact way to review accumulated evidence before proposing stronger runtime
defaults. P65 adds a read-only review report that summarizes adoption events by
track, feature, and signal so future defaulting specs can cite concrete evidence
instead of raw event lists.

## Decisions

- Add CLI `mempal phase3 adoption review`.
- Add MCP `mempal_phase3 action=review`.
- Review accepts optional `track`, `feature`, `signal`, `limit`, and `format`
  filters.
- Review returns `writes=false`, applied filters, aggregate signal counts,
  per-feature signal counts, `conclusion`, and `reasons`.
- `signal` filtering is applied after DB retrieval; it must not require schema
  changes or new indexes.
- Review is read-only and must not append runtime adoption events.
- Review conclusions are advisory: `no_evidence`, `positive`, `rollback_risk`,
  or `mixed`.

## Boundaries

### Allowed Changes
- `src/core/phase3.rs`
- `src/main.rs`
- `src/mcp/**`
- `src/core/protocol.rs`
- `tests/phase3_runtime.rs`
- `tests/knowledge_card_retrieval.rs` (test harness timeout robustness only)
- `docs/MIND-MODEL-DESIGN.md`
- `docs/plans/2026-05-13-p65-runtime-adoption-review-report.md`
- `specs/p65-runtime-adoption-review-report.spec.md`
- `AGENTS.md`
- `CLAUDE.md`

### Forbidden
- Do not automatically record events.
- Do not add hooks, background workers, or implicit runtime instrumentation.
- Do not change schema v9.
- Do not change Phase-3 gate thresholds.
- Do not make card context default.
- Do not add card embeddings.
- Do not treat review conclusions as authority to mutate knowledge lifecycle or
  runtime defaults.

## Acceptance Criteria

Scenario: CLI review summarizes card context evidence
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_review_json_summarizes_events
  Level: integration
  Given an empty CLI HOME
  And card context events include accepted and rejected signals
  When running `mempal phase3 adoption review --track card_context --format json`
  Then stdout is valid JSON
  And the response includes `writes=false`
  And `total` is 3
  And `stats.accepted` is 2
  And `stats.rejected` is 1
  And `features[0].feature` is `include_cards`
  And runtime adoption event count remains 3

Scenario: CLI review supports signal filtering
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_review_json_filters_signal
  Level: integration
  Given an empty CLI HOME
  And mixed card context events exist
  When running `mempal phase3 adoption review --track card_context --signal accepted --format json`
  Then stdout is valid JSON
  And `total` is 2
  And `stats.accepted` is 2
  And `stats.rejected` is 0

Scenario: CLI review reports no evidence without mutation
  Test:
    Package: mempal
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_review_json_no_evidence_read_only
  Level: integration
  Given an empty CLI HOME
  When running `mempal phase3 adoption review --track evaluator --format json`
  Then stdout is valid JSON
  And `writes=false`
  And `total` is 0
  And `conclusion` is `no_evidence`
  And runtime adoption event count remains zero

Scenario: MCP review is read-only
  Test:
    Package: mempal
    Filter: mcp::server::tests::test_mcp_phase3_review_action_is_read_only
  Level: unit
  Given an empty test database with one runtime adoption event
  When `mempal_phase3` is called with `action=review`
  Then the response includes `writes=false`
  And the response includes a review report
  And runtime adoption event count remains unchanged

## Out of Scope

- New write paths or automatic event capture.
- New DB tables, migrations, or indexes.
- Changing readiness gate thresholds.
- Enabling card context, card embeddings, evaluator authority, or research
  ingestion by default.
