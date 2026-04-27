spec: task
name: "P43: knowledge card retrieval contract"
inherits: project
tags: [memory, mind-model, knowledge-cards, retrieval, phase-2]
---

## Intent

P43 defines the read contract for consuming Phase-2 knowledge cards at runtime
without changing default search or context behavior. The contract clarifies what
a card retrieval result must contain, how linked evidence citations are exposed,
and which card statuses are eligible for runtime use.

## Decisions

- Define a card retrieval item as a governed knowledge result, not a raw drawer result.
- Each retrieval item must include `card_id`, `statement`, `content`, `tier`, `status`, `domain`, `field`, `anchor_kind`, and `anchor_id`.
- Each retrieval item must expose role-separated evidence citations from `knowledge_evidence_links`.
- Evidence citations must include `evidence_drawer_id`, `role`, and the evidence drawer's `source_file`.
- Runtime-eligible card statuses are only `promoted` and `canonical` by default.
- `candidate`, `demoted`, and `retired` cards must not be returned by default runtime retrieval.
- P43 is contract-only: do not change `mempal context`, `mempal_context`, or `mempal_search` default behavior.
- P43 does not define card embeddings; retrieval ranking strategy is deferred to later implementation specs.

## Acceptance Criteria

Scenario: contract defines card retrieval item fields
  Test:
    Package: mempal
    Filter: rg -n "card_id.*statement.*content|evidence_drawer_id.*role.*source_file" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 retrieval contract section
  Then it lists the required card fields
  And it lists the required evidence citation fields

Scenario: contract defines default runtime status eligibility
  Test:
    Package: mempal
    Filter: rg -n "promoted.*canonical|candidate.*demoted.*retired" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 retrieval contract section
  Then it states that promoted and canonical cards are runtime-eligible by default
  And it states that candidate, demoted, and retired cards are excluded by default

Scenario: contract keeps default context and search unchanged
  Test:
    Package: mempal
    Filter: rg -n "P43 does not change.*mempal context|P43 does not change.*mempal_search" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 retrieval contract section
  Then it explicitly says P43 does not change default context or search behavior

Scenario: inventory includes P43 contract
  Test:
    Package: mempal
    Filter: rg -n "p43-knowledge-card-retrieval-contract" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P43
  Then both inventories list it as a contract-only completed spec

Scenario: no card runtime implementation is added in P43
  Test:
    Package: mempal
    Filter: git diff --name-only main...HEAD
  Given the P43 branch
  When reviewing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs
  And no Rust source or test file is changed

## Out of Scope

- Do not implement card retrieval APIs.
- Do not add card embeddings.
- Do not change `mempal context`.
- Do not change `mempal_context`.
- Do not change `mempal_search`.
- Do not change MCP tools.
- Do not change REST endpoints.

## Constraints

- The contract must preserve evidence drawers as the citation root.
- The contract must not imply cards replace drawers as default runtime source.
- The contract must leave ranking strategy to later specs.
