spec: task
name: "P30: Phase-2 knowledge card storage boundary"
inherits: project
tags: [memory, mind-model, knowledge-cards, storage, design]
---

## Intent

Phase 1 stores evidence and bootstrap knowledge in `drawers`. The remaining
MIND-MODEL-DESIGN question is where Phase-2 `knowledge_cards` should live when
knowledge is structurally separated from evidence. This task resolves the
design boundary without implementing the Phase-2 schema.

## Decisions

- Phase-2 `knowledge_cards` will live in the same SQLite `palace.db`.
- Phase-2 must use separate tables such as `knowledge_cards`,
  `knowledge_evidence_links`, and `knowledge_events`.
- `drawers` remain the evidence/raw-text store and citation root.
- Knowledge cards reference evidence drawers by `drawer_id`; they do not copy
  or replace evidence content.
- Knowledge-card lifecycle events stay transactional with the evidence links
  they modify.
- Do not introduce a separate persistence layer unless a future measured need
  proves the single-file SQLite boundary insufficient.
- P30 is design-only: it does not add tables, migrations, CLI commands, MCP
  tools, or REST APIs.

## Acceptance Criteria

Scenario: MIND-MODEL-DESIGN records the storage decision
  Test: docs_p30_mind_model_records_same_db_storage
  Given the MIND-MODEL-DESIGN Phase-2 section
  When running `rg -n "same SQLite .*palace.db|knowledge_cards.*same SQLite" docs/MIND-MODEL-DESIGN.md`
  Then it states that Phase-2 knowledge cards live in the same SQLite palace.db
  And it still keeps knowledge tables structurally separate from evidence drawers

Scenario: MIND-MODEL-DESIGN has no remaining open questions
  Test: docs_p30_mind_model_has_no_open_questions
  Given the MIND-MODEL-DESIGN document
  When running `! rg -n "^## Open Questions|should knowledge cards live" docs/MIND-MODEL-DESIGN.md`
  Then no Open Questions section remains
  And the previous knowledge-card storage question is not left unresolved

Scenario: usage docs state Phase 2 is not implemented yet
  Test: docs_p30_usage_states_phase2_not_implemented
  Given the usage guide
  When running `rg -n "Phase-2 .*same SQLite palace.db|not implemented" docs/usage.md`
  Then it states that Phase-2 knowledge cards are a future same-DB table split
  And it does not imply the Phase-2 tables already exist

Scenario: task inventory marks P30 complete
  Test: docs_p30_agent_inventories_mark_complete
  Given repository agent instructions
  When running `rg -n "p30-knowledge-card-storage-boundary|P30 knowledge card storage boundary" AGENTS.md CLAUDE.md`
  Then P30 is listed as completed
  And its plan is listed with the completed implementation plans

Scenario: Phase-2 implementation remains out of scope
  Test: docs_p30_no_phase2_runtime_surface_added
  Given this design-only task
  When running `! rg -n "CREATE TABLE knowledge_cards|struct KnowledgeCard|mempal_knowledge_card" src tests`
  Then no Phase-2 knowledge card schema or runtime surface has been added

## Out of Scope

- Do not add schema migrations.
- Do not create `knowledge_cards`, `knowledge_evidence_links`, or
  `knowledge_events` tables yet.
- Do not add `KnowledgeCard` Rust types.
- Do not add CLI, MCP, or REST surfaces for knowledge cards.
- Do not migrate existing knowledge drawers.
- Do not change context assembly, wake-up, search, ranking, or promotion gates.
- Do not split persistence into a second database or external service.

## Constraints

- Preserve the single-binary, single-file local memory model.
- Preserve raw evidence citations through existing drawer ids.
- Treat same-DB Phase 2 as a future schema evolution, not a P30 implementation.
