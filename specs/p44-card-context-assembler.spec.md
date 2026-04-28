spec: task
name: "P44: card-aware context assembler"
inherits: project
tags: [memory, mind-model, knowledge-cards, context, phase-2]
---

## Intent

P44 adds an explicit opt-in path for `mempal context` and `mempal_context` to
include Phase-2 knowledge cards according to the P43 retrieval contract. Default
context behavior remains drawer-only.

## Decisions

- Add `include_cards` to the core context request, CLI, and MCP request.
- Default `include_cards` is false for CLI and MCP.
- When enabled, only `promoted` and `canonical` cards are included.
- Card context items include `card_id` and role-separated evidence citations.
- Card evidence citations include `evidence_drawer_id`, `role`, and `source_file`.
- Cards are appended within the existing `dao_tian -> dao_ren -> shu -> qi` sections.
- P44 does not add card embeddings or semantic card ranking.
- P44 does not change `mempal_search`.

## Acceptance Criteria

Scenario: default context remains drawer-only
  Test:
    Package: mempal
    Filter: test_context_omits_cards_by_default
  Given a matching active knowledge card
  When assembling context without include_cards
  Then no context item contains card_id

Scenario: include_cards adds active cards with evidence citations
  Test:
    Package: mempal
    Filter: test_context_include_cards_adds_active_card_citations
  Given active and inactive cards with evidence links
  When assembling context with include_cards=true
  Then only active cards are included
  And each card item includes evidence citation fields

Scenario: CLI include-cards JSON exposes card metadata
  Test:
    Package: mempal
    Filter: test_cli_context_include_cards_json
  Given a matching promoted card with evidence
  When running `mempal context <query> --include-cards --format json`
  Then JSON includes card_id and evidence_citations

Scenario: MCP include_cards exposes card metadata
  Test:
    Package: mempal
    Filter: test_mcp_context_include_cards_appends_card_items
  Given a matching promoted card with evidence
  When calling `mempal_context` with include_cards=true
  Then response includes a card context item with evidence citations

Scenario: search remains unchanged
  Test:
    Package: mempal
    Filter: rg -n "include_cards" src/search tests/search_neighbors.rs
  Given P44 changes
  When searching search implementation files
  Then include_cards is absent from search code

## Out of Scope

- Do not add card embeddings.
- Do not add card semantic ranking.
- Do not change `mempal_search`.
- Do not make cards default context source.
- Do not remove Stage-1 drawer context.

## Constraints

- Evidence drawers remain the citation root.
- Existing context JSON fields must remain backward compatible.
- Card items must not include candidate, demoted, or retired cards by default.
