spec: task
name: "P34: knowledge card CLI management"
inherits: project
tags: [memory, mind-model, knowledge-cards, cli, phase-2]
---

## Intent

P33 added the DB-layer API for Phase-2 knowledge cards. P34 exposes a minimal
operator-facing CLI surface so cards can be created, inspected, filtered, linked
to evidence drawers, and annotated with append-only lifecycle events without
adding MCP, REST, search, context, or backfill behavior.

## Decisions

- Add a new top-level `mempal knowledge-card` command family.
- Add `knowledge-card create` for inserting a single card.
- Add `knowledge-card get` for inspecting one card.
- Add `knowledge-card list` for filtered card listing.
- Add `knowledge-card link` for linking one evidence drawer to one card.
- Add `knowledge-card events` for listing a card's append-only event history.
- Add `knowledge-card event` for appending one lifecycle event.
- Keep stdout format support minimal: plain by default, `--format json` where a
  command returns structured records.
- Generate deterministic ids when `--id` is omitted; callers may also provide
  explicit ids for reproducible tests and migrations.
- `knowledge-card link` must reuse P33 validation and reject non-evidence
  drawers before writing.
- Do not add update or delete CLI commands in P34.
- Do not add MCP tools, REST endpoints, search integration, context integration,
  wake-up behavior, lifecycle automation, promotion gates, or backfill behavior.

## Acceptance Criteria

Scenario: create and get card through CLI
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_create_get_json
  Given an initialized mempal home
  When running `mempal knowledge-card create` with explicit id and metadata
  Then stdout includes `card_id=card_cli`
  When running `mempal knowledge-card get card_cli --format json`
  Then the JSON contains the same statement, content, tier, status, domain, field, and anchor metadata

Scenario: create generates deterministic id when omitted
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_create_generates_id
  Given an initialized mempal home
  When running `mempal knowledge-card create` without `--id`
  Then stdout includes `card_id=card_`
  And the generated card can be loaded with `mempal knowledge-card get`

Scenario: list cards supports filters
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_list_filters_plain
  Given multiple knowledge cards with different tier, status, domain, field, and anchor metadata
  When running `mempal knowledge-card list --tier dao_ren --status promoted --field rust`
  Then stdout includes only the matching card id and excludes non-matching cards

Scenario: link rejects non-evidence drawers
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_link_requires_evidence_drawer
  Given one knowledge card, one evidence drawer, and one knowledge drawer
  When running `mempal knowledge-card link card_cli drawer_evidence --role supporting`
  Then the link succeeds and stdout includes `link_id=`
  When running `mempal knowledge-card link card_cli drawer_knowledge --role supporting`
  Then the command exits non-zero
  And stderr says the target must be an evidence drawer

Scenario: event append and events list roundtrip JSON
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_event_append_and_list_json
  Given one knowledge card
  When running `mempal knowledge-card event card_cli --type created --reason seeded`
  Then stdout includes `event_id=`
  When running `mempal knowledge-card events card_cli --format json`
  Then the JSON array contains the created event and reason

Scenario: invalid format is rejected
  Test:
    Package: mempal
    Filter: test_cli_knowledge_card_rejects_invalid_format
  Given an initialized mempal home
  When running `mempal knowledge-card list --format yaml`
  Then the command exits non-zero
  And stderr says the format is unsupported

## Out of Scope

- Do not add card update CLI.
- Do not add card delete CLI.
- Do not add MCP tools.
- Do not add REST endpoints.
- Do not change search results.
- Do not change context assembly.
- Do not change wake-up.
- Do not mutate Stage-1 `drawers` knowledge lifecycle commands.
- Do not backfill existing `memory_kind=knowledge` drawers into cards.

## Constraints

- Store cards in schema v8 `knowledge_cards`.
- Preserve raw evidence in `drawers`; CLI links cards to evidence drawer ids.
- Keep lifecycle history append-only by only inserting `knowledge_events`.
- Reuse the P33 `Database` APIs instead of writing direct SQL in CLI command functions.
