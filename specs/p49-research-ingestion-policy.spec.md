spec: task
name: "P49: research ingestion policy"
inherits: project
tags: [mind-model, research, ingestion-policy]
---

## Intent

External `research-rs` output should feed the memory system, but it must not
become a parallel authority for `dao`. P49 closes this design item by defining
the allowed path: research outputs enter as evidence drawers or candidate
insights backed by evidence refs; promotion into `dao` remains governed by the
memory lifecycle gates.

## Decisions

- `research-rs` remains external `qi`, not a `dao` container.
- Research raw/source output enters mempal as `memory_kind=evidence` with `provenance=research`.
- Research summaries or insights may become candidate knowledge only through distill from existing evidence refs.
- Research must not directly create `dao_tian`, `canonical`, or `promoted` knowledge.
- Contradiction signals from research should be stored as evidence/counterexamples for later demotion or gate evaluation.
- P49 is policy-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p49-research-ingestion-policy.spec.md
- docs/plans/2026-04-29-p49-research-ingestion-policy.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not add a research-rs adapter or CLI command.
- Do not change ingest, distill, promotion, or demotion behavior.
- Do not allow research to bypass lifecycle gates.

## Acceptance Criteria

Scenario: MIND-MODEL records research ingestion path
  Test:
    Filter: rg -n "P49 defines the research-rs ingestion path|memory_kind=evidence.*provenance=research|candidate knowledge only through distill" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the research-rs boundary section
  Then it states that research output enters as evidence with research provenance
  And it states that candidate knowledge must be created through distill from evidence refs

Scenario: MIND-MODEL forbids research-defined dao
  Test:
    Filter: rg -n "research must not directly create dao_tian|must not directly create canonical or promoted knowledge|lifecycle gates" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the research-rs boundary section
  Then it states that research must not directly create dao_tian
  And it states research cannot bypass lifecycle gates

Scenario: Inventories include P49
  Test:
    Filter: rg -n "p49-research-ingestion-policy|P49 research ingestion policy" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P49
  Then both AGENTS.md and CLAUDE.md include the P49 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P49 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Existing runtime support remains sufficient for policy
  Test:
    Filter: rg -n "Provenance::Research|distill only allows candidate dao_ren or qi|evidence drawer accepts explicit runtime or research provenance" src tests
  Given existing runtime behavior
  When searching for research provenance and distill constraints
  Then the existing code supports research evidence
  And distill remains constrained to candidate dao_ren or qi

## Out of Scope

- Implementing a research-rs import adapter.
- Adding new schema for research reports.
- Changing promotion thresholds.
- Allowing automatic dao creation from research output.
