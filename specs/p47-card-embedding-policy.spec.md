spec: task
name: "P47: card embedding policy"
inherits: project
tags: [mind-model, knowledge-card, retrieval-policy]
---

## Intent

P45 implemented linked-evidence-first card retrieval. P47 resolves the remaining
card embedding question: do not add card-level embeddings yet; define the evidence
and schema constraints required before a later implementation may introduce them.

## Decisions

- Keep P45 linked-evidence retrieval as the only implemented card retrieval strategy for now.
- Do not add `knowledge_card_vectors`, card vector writes, card reindexing, or card embedding ranking in P47.
- Card embeddings are only justified if linked-evidence retrieval misses useful cards because the evidence wording does not match the user query but the card statement does.
- Any future card embedding implementation must preserve linked evidence citations as the citation root.
- Any future card embedding implementation must define schema migration, reindex behavior, stale-vector handling, and rollback behavior.
- P47 is policy-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p47-card-embedding-policy.spec.md
- docs/plans/2026-04-28-p47-card-embedding-policy.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not add migrations or schema changes.
- Do not add card vector tables.
- Do not change `mempal_search`, `mempal_context`, or `mempal_knowledge_cards` behavior.

## Acceptance Criteria

Scenario: MIND-MODEL records no-card-embedding decision
  Test:
    Filter: rg -n "P47 keeps card-level embeddings deferred|linked-evidence retrieval remains the only implemented card retrieval strategy" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 card retrieval policy
  Then it states that P47 defers card-level embeddings
  And it states that linked-evidence retrieval remains the implemented strategy

Scenario: MIND-MODEL defines evidence required for future card embeddings
  Test:
    Filter: rg -n "Evidence required before card embeddings|statement-match misses|stale-vector handling|rollback behavior" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the card embedding policy
  Then it lists evidence required before card embeddings
  And it mentions stale-vector handling and rollback behavior

Scenario: Inventories include P47
  Test:
    Filter: rg -n "p47-card-embedding-policy|P47 card embedding policy" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P47
  Then both AGENTS.md and CLAUDE.md include the P47 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P47 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Vector schema remains absent
  Test:
    Filter: "! rg -n \"knowledge_card_vectors|card vector|card_vectors\" src"
  Given the P47 branch
  When searching runtime source
  Then no card vector schema or runtime implementation exists

## Out of Scope

- Card embedding implementation.
- Card vector table migration.
- Card reindex command.
- Hybrid card embedding plus linked-evidence ranking.
