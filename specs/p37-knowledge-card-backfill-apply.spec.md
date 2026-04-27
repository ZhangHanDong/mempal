spec: task
name: "P37: knowledge card backfill apply"
inherits: project
tags: [memory, mind-model, knowledge-cards, backfill, phase-2]
---

## Intent

P36 added a read-only backfill report from Stage-1 knowledge drawers to Phase-2
knowledge cards. P37 adds the explicit apply operation that can materialize
ready candidates as cards, evidence links, and created events while remaining
safe by default.

## Decisions

- Add a core `apply_backfill` function that consumes the P36 report.
- Add a CLI command `mempal knowledge-card backfill-apply`.
- The CLI defaults to dry-run; it writes only when `--execute` is provided.
- Dry-run output must show the same counts plus `created_count=0`.
- Execute mode inserts one `knowledge_cards` row per `ready` candidate.
- Execute mode inserts evidence links from the source drawer's supporting,
  verification, counterexample, and teaching refs.
- Evidence links are inserted only for refs that resolve to existing evidence
  drawers; invalid refs are reported in `link_errors` and do not abort the card
  insert.
- Execute mode appends one `created` event per created card.
- Already-existing and skipped candidates are not mutated.
- Re-running execute mode must be idempotent: previously created cards become
  `already_exists` and are not duplicated.
- Support the same filters as `backfill-plan` and the same plain / JSON formats.
- Do not delete or mutate Stage-1 knowledge drawers in P37.
- Do not add MCP, REST, search, context, wake-up, lifecycle, or promotion-gate
  behavior in P37.

## Acceptance Criteria

Scenario: core apply dry-run has no database side effects
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_apply_dry_run_no_side_effects
  Given one ready Stage-1 knowledge drawer with evidence refs
  When applying backfill with execute=false
  Then no card, evidence link, event, drawer, vector, or audit row is written
  And output reports created_count=0

Scenario: core apply execute creates card links and event
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_apply_execute_creates_card_links_event
  Given one ready Stage-1 knowledge drawer with supporting, verification, counterexample, and teaching evidence refs
  When applying backfill with execute=true
  Then one knowledge card is created from drawer metadata
  And evidence links are created with the correct roles
  And one created event is appended

Scenario: execute is idempotent
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_apply_execute_is_idempotent
  Given one ready Stage-1 knowledge drawer
  When applying backfill with execute=true twice
  Then the second run creates zero cards
  And card, link, and event counts do not increase

Scenario: invalid evidence refs are reported without aborting card insert
  Test:
    Package: mempal
    Filter: test_knowledge_card_backfill_apply_reports_invalid_evidence_refs
  Given one ready Stage-1 knowledge drawer with one valid evidence ref and one invalid ref
  When applying backfill with execute=true
  Then the card is created
  And the valid evidence link is created
  And the invalid ref appears in link_errors

Scenario: CLI backfill-apply defaults to dry-run
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_apply_defaults_to_dry_run
  Given one ready Stage-1 knowledge drawer
  When running `mempal knowledge-card backfill-apply`
  Then stdout says `dry_run=true`
  And no knowledge card is written

Scenario: CLI backfill-apply execute JSON supports filters
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_apply_execute_json_filters
  Given ready Stage-1 knowledge drawers across multiple fields
  When running `mempal knowledge-card backfill-apply --execute --field rust --format json`
  Then the JSON output reports created_count=1
  And only the matching field is materialized as a card

Scenario: command rejects unsupported format
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_backfill_apply_rejects_invalid_format
  Given an initialized mempal home
  When running `mempal knowledge-card backfill-apply --format yaml`
  Then the command exits non-zero
  And stderr says the format is unsupported

## Out of Scope

- Do not delete or rewrite Stage-1 knowledge drawers.
- Do not migrate vectors.
- Do not write the JSONL audit log.
- Do not add MCP tools.
- Do not add REST endpoints.
- Do not change search results.
- Do not change context assembly.
- Do not change wake-up.
- Do not change Stage-1 lifecycle commands.

## Constraints

- Execute mode must use deterministic prospective card ids from P36.
- Apply must never operate on skipped or already-existing candidates.
- Card creation and event creation for a single candidate should be transactional.
- Link failures must be observable but must not roll back the already-created card.
