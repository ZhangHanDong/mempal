spec: task
name: "P48: card audit policy"
inherits: project
tags: [mind-model, knowledge-card, audit-policy]
---

## Intent

Phase-2 knowledge cards already have transactional append-only
`knowledge_events`. P48 resolves whether card lifecycle should also write the
global JSONL audit log: no default double-write; keep `knowledge_events` as the
authoritative card lifecycle audit trail, and only add JSONL export later if an
external integration needs it.

## Decisions

- `knowledge_events` remain the authoritative audit trail for Phase-2 card lifecycle.
- Do not write `audit.jsonl` from card promote/demote/backfill lifecycle by default.
- Keep Stage-1 drawer lifecycle JSONL audit behavior unchanged.
- If an external tool needs JSONL card history, add an explicit export surface rather than transactional dual-write.
- Any future JSONL export must be derived from `knowledge_events` and must not become a second source of truth.
- P48 is policy-only and must not change Rust runtime behavior.

## Boundaries

### Allowed Changes
- specs/p48-card-audit-policy.spec.md
- docs/plans/2026-04-29-p48-card-audit-policy.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not modify `src/**`.
- Do not change card promote/demote/backfill behavior.
- Do not write new JSONL audit entries for card lifecycle.
- Do not change `knowledge_events` schema.

## Acceptance Criteria

Scenario: MIND-MODEL records card audit decision
  Test:
    Filter: rg -n "P48 keeps knowledge_events as the authoritative Phase-2 card audit trail|no default JSONL dual-write" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the Phase-2 card audit policy
  Then it states that `knowledge_events` are authoritative
  And it states there is no default JSONL dual-write

Scenario: MIND-MODEL defines future JSONL export boundary
  Test:
    Filter: rg -n "JSONL export.*derived from knowledge_events|not become a second source of truth" docs/MIND-MODEL-DESIGN.md
  Given the MIND-MODEL design document
  When reading the card audit policy
  Then it states future JSONL support should be export-derived
  And it states JSONL must not become a second source of truth

Scenario: Inventories include P48
  Test:
    Filter: rg -n "p48-card-audit-policy|P48 card audit policy" AGENTS.md CLAUDE.md
  Given repo agent inventories
  When searching for P48
  Then both AGENTS.md and CLAUDE.md include the P48 spec and plan entries

Scenario: Runtime source files are unchanged
  Test:
    Filter: git diff --name-only main...HEAD
  Given the P48 branch
  When listing changed files
  Then changes are limited to spec, plan, MIND-MODEL design, and agent inventory docs

Scenario: Existing no-JSONL card lifecycle rule remains visible
  Test:
    Filter: rg -n "Do not write JSONL audit logs for Phase-2 card lifecycle" specs/p39-knowledge-card-lifecycle-cli.spec.md
  Given existing card lifecycle specs
  When reading P39
  Then the original no-JSONL rule remains present

## Out of Scope

- JSONL export implementation.
- Changing card lifecycle runtime behavior.
- Changing Stage-1 drawer audit behavior.
- Changing `knowledge_events` schema.
