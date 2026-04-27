spec: task
name: "P31: Phase-2 knowledge card schema contract"
inherits: project
tags: [memory, mind-model, knowledge-cards, schema, phase-2]
---

## Intent

P30 fixed the storage boundary: Phase-2 knowledge cards belong in the same
SQLite `palace.db`, but in tables separate from raw evidence drawers. P31
defines the minimum schema contract for those future tables so the following
migration task can implement schema v8 without re-opening the model shape.

## Decisions

- Phase-2 implementation target is schema v8.
- Add table `knowledge_cards` for beliefs / reusable knowledge.
- Add table `knowledge_evidence_links` for typed links from cards to evidence
  drawers.
- Add table `knowledge_events` for append-only lifecycle and governance events.
- `drawers` remain the raw evidence and citation root.
- `knowledge_cards` must not copy raw evidence content from `drawers.content`.
- `knowledge_evidence_links.evidence_drawer_id` references `drawers.id`.
- `knowledge_events.card_id` references `knowledge_cards.id`.
- Initial `knowledge_cards` columns:
  - `id TEXT PRIMARY KEY`
  - `statement TEXT NOT NULL`
  - `content TEXT NOT NULL`
  - `tier TEXT NOT NULL CHECK ('qi','shu','dao_ren','dao_tian')`
  - `status TEXT NOT NULL CHECK ('candidate','promoted','canonical','demoted','retired')`
  - `domain TEXT NOT NULL CHECK ('project','agent','skill','global')`
  - `field TEXT NOT NULL DEFAULT 'general'`
  - `anchor_kind TEXT NOT NULL CHECK ('global','repo','worktree')`
  - `anchor_id TEXT NOT NULL`
  - `parent_anchor_id TEXT`
  - `scope_constraints TEXT`
  - `trigger_hints TEXT`
  - `created_at TEXT NOT NULL`
  - `updated_at TEXT NOT NULL`
- Initial `knowledge_evidence_links` columns:
  - `id TEXT PRIMARY KEY`
  - `card_id TEXT NOT NULL`
  - `evidence_drawer_id TEXT NOT NULL`
  - `role TEXT NOT NULL CHECK ('supporting','verification','counterexample','teaching')`
  - `note TEXT`
  - `created_at TEXT NOT NULL`
- Initial `knowledge_events` columns:
  - `id TEXT PRIMARY KEY`
  - `card_id TEXT NOT NULL`
  - `event_type TEXT NOT NULL CHECK ('created','promoted','demoted','retired','linked','unlinked','updated','published_anchor')`
  - `from_status TEXT`
  - `to_status TEXT`
  - `reason TEXT NOT NULL`
  - `actor TEXT`
  - `metadata TEXT`
  - `created_at TEXT NOT NULL`
- `knowledge_evidence_links` must enforce `UNIQUE(card_id, evidence_drawer_id, role)`.
- The schema must index card lookup by `(tier, status)`, `(domain, field)`, and
  `(anchor_kind, anchor_id)`.
- The schema must index evidence links by `card_id` and `evidence_drawer_id`.
- The schema must index events by `card_id, created_at`.
- P31 is spec-only and documentation-only; it must not add schema migrations,
  Rust structs, CLI commands, MCP tools, REST APIs, or context behavior.

## Acceptance Criteria

Scenario: schema contract is parseable
  Test: p31_agent_spec_parse
  Given `specs/p31-knowledge-card-schema.spec.md`
  When running `agent-spec parse specs/p31-knowledge-card-schema.spec.md`
  Then the spec parses successfully
  And it lists scenarios for all three Phase-2 tables

Scenario: schema contract passes lint
  Test: p31_agent_spec_lint
  Given `specs/p31-knowledge-card-schema.spec.md`
  When running `agent-spec lint specs/p31-knowledge-card-schema.spec.md --min-score 0.7`
  Then the lint score is at least 0.7

Scenario: MIND-MODEL-DESIGN includes the minimum schema draft
  Test: docs_p31_mind_model_lists_phase2_tables
  Given the MIND-MODEL-DESIGN Phase-2 section
  When running `rg -n "knowledge_cards|knowledge_evidence_links|knowledge_events|schema v8" docs/MIND-MODEL-DESIGN.md`
  Then all three Phase-2 table names are present
  And the document states that schema v8 is the implementation target

Scenario: repository inventory lists P31 as current draft
  Test: docs_p31_agent_inventory_lists_current_spec
  Given repository agent instructions
  When running `rg -n "p31-knowledge-card-schema|schema v8|未实现" AGENTS.md CLAUDE.md`
  Then P31 appears under current draft specs
  And P31 does not appear in the completed spec table

Scenario: no Phase-2 schema implementation is added in P31
  Test: docs_p31_no_schema_runtime_added
  Given this spec-only task
  When running `! rg -n "CREATE TABLE knowledge_cards|struct KnowledgeCard|mempal_knowledge_card" src tests`
  Then no schema migration, Rust type, or runtime surface has been added

Scenario: future schema must reject invalid evidence link roles
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_links_reject_invalid_role
  Given future schema v8 with `knowledge_evidence_links`
  When inserting a link with `role='invalid_role'`
  Then SQLite rejects the row through a CHECK constraint

Scenario: future schema must deduplicate evidence links per role
  Test:
    Package: mempal
    Filter: test_knowledge_evidence_links_dedup_card_drawer_role
  Given future schema v8 with one card and one evidence drawer
  When inserting the same `(card_id, evidence_drawer_id, role)` twice
  Then the second insert fails through a UNIQUE constraint

Scenario: future schema must keep events append-only
  Test:
    Package: mempal
    Filter: test_knowledge_events_are_append_only
  Given future schema v8 with one card
  When lifecycle changes occur
  Then each change appends a new `knowledge_events` row
  And previous event rows are not updated in place

## Out of Scope

- Do not add schema v8 migration in P31.
- Do not add `knowledge_cards`, `knowledge_evidence_links`, or
  `knowledge_events` tables in P31.
- Do not add Rust structs or DB APIs.
- Do not add CLI, MCP, or REST surfaces.
- Do not change `mempal context`, `wake-up`, search, promotion gates, or
  lifecycle behavior.
- Do not migrate existing `memory_kind=knowledge` drawers.
- Do not decide the final backfill algorithm.

## Constraints

- Keep raw evidence in `drawers`; knowledge cards reference it by drawer id.
- Keep card lifecycle auditable through append-only events.
- Keep schema v8 compatible with the current single-file `palace.db` model.
- Preserve existing Stage-1 tier/status vocabulary unless a later spec changes
  it explicitly.
