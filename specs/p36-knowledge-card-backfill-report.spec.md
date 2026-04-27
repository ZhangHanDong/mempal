spec: task
name: "P36: knowledge card backfill dry-run report"
inherits: project
tags: [memory, mind-model, knowledge-cards, backfill, phase-2]
---

## Intent

P32-P35 made Phase-2 knowledge cards usable, but existing Stage-1 knowledge still
lives in `drawers` with `memory_kind=knowledge`. P36 adds a read-only backfill
planning report that maps eligible knowledge drawers to prospective card ids and
explains skips before any migration command exists.

## Decisions

- Add a core report function for Stage-1 knowledge drawer to Phase-2 card planning.
- Add a CLI command `mempal knowledge-card backfill-plan`.
- Keep the command dry-run only; it never writes `knowledge_cards`,
  `knowledge_evidence_links`, `knowledge_events`, `drawers`, vectors, or audit logs.
- Generate prospective card ids deterministically from the source drawer id.
- Mark drawers as `ready` only when required card fields are present:
  statement, tier, status, domain, field, anchor_kind, and anchor_id.
- Mark drawers as `already_exists` when the prospective card id is already present.
- Mark drawers as `skipped` with reasons when required fields are missing or invalid.
- Support optional filters for domain, field, tier, status, and anchor_kind.
- Support plain output and `--format json`.
- Do not add actual backfill migration, MCP, REST, search, context, wake-up, or
  lifecycle behavior in P36.

## Acceptance Criteria

Scenario: core report classifies ready skipped and existing drawers
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_report_classifies_drawers
  Given Stage-1 knowledge drawers covering ready, missing metadata, and already-carded cases
  When building the backfill report
  Then ready drawers include prospective card ids
  And skipped drawers include explicit reasons
  And existing card rows are reported as already_exists

Scenario: core report is read-only
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_report_has_no_db_side_effects
  Given a database with knowledge drawers and no card rows
  When building the backfill report repeatedly
  Then drawer count and knowledge card count stay unchanged

Scenario: CLI plain report summarizes counts and items
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_plan_plain
  Given an initialized mempal home with Stage-1 knowledge drawers
  When running `mempal knowledge-card backfill-plan`
  Then stdout includes ready, skipped, and already_exists counts
  And stdout includes the source drawer id and prospective card id

Scenario: CLI JSON report supports filters
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_plan_json_filters
  Given knowledge drawers across multiple fields
  When running `mempal knowledge-card backfill-plan --field rust --format json`
  Then the JSON report contains only matching drawer candidates

Scenario: command rejects unsupported format
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_plan_rejects_invalid_format
  Given an initialized mempal home
  When running `mempal knowledge-card backfill-plan --format yaml`
  Then the command exits non-zero
  And stderr says the format is unsupported

## Out of Scope

- Do not insert knowledge cards.
- Do not insert evidence links.
- Do not insert knowledge events.
- Do not mutate drawers.
- Do not re-embed vectors.
- Do not write audit logs.
- Do not add MCP tools.
- Do not add REST endpoints.
- Do not change search, context, wake-up, or lifecycle behavior.

## Constraints

- The report must use existing Stage-1 drawer metadata as source of truth.
- Missing required metadata must be visible in report output.
- Deterministic prospective ids must allow future migration to be idempotent.
