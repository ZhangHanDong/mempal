spec: task
name: "P19: lifecycle evidence ref validation"
inherits: project
tags: [memory, knowledge, lifecycle, validation]
estimate: 0.25d
---

## Intent

P17 added manual knowledge lifecycle commands, and P18 made `distill` require
role refs to point to evidence drawers. P19 applies the same evidence discipline
to `promote` and `demote`, so lifecycle state changes cannot be justified by
missing refs, malformed ids, or knowledge drawers masquerading as evidence.

This is a Stage-1 governance hardening task. It does not introduce evaluator
scoring, MCP/REST lifecycle endpoints, schema changes, or Phase-2 knowledge
cards.

## Decisions

- `mempal knowledge promote` `--verification-ref` values must be evidence drawer ids
- `mempal knowledge demote` `--evidence-ref` values must be evidence drawer ids
- A valid lifecycle evidence ref must:
  - start with `drawer_`
  - exist in the database
  - have evidence memory kind
- The existing requirement for at least one lifecycle ref remains unchanged
- Refs are still appended with stable de-duplication after validation
- Error messages should distinguish malformed ids, missing drawers, and wrong drawer kind
- No MCP / REST lifecycle endpoint

## Boundaries

### Allowed Changes
- `src/main.rs`
- `tests/knowledge_lifecycle.rs`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/usage.md`
- `docs/MIND-MODEL-DESIGN.md`
- `specs/p19-lifecycle-ref-validation.spec.md`
- `docs/plans/2026-04-24-p19-lifecycle-ref-validation-implementation.md`

### Forbidden
- Do not add tables, columns, triggers, or migrations
- Do not add Phase-2 `knowledge_cards`
- Do not add MCP / REST lifecycle endpoints
- Do not change `mempal_context` response schema or ordering
- Do not change search ranking
- Do not introduce new dependencies

## Out of Scope

- evaluator scoring
- automatic promotion or demotion
- human review UI
- lifecycle event table
- research-rs integration
- ref role inference

## Completion Criteria

Scenario: promote rejects malformed verification refs
  Test:
    Filter: test_cli_knowledge_promote_rejects_malformed_verification_ref
    Level: integration
  Given a candidate knowledge drawer exists
  When running `mempal knowledge promote <id> --status promoted --verification-ref not_a_drawer --reason bad`
  Then the command fails
  And the error says lifecycle refs must contain drawer ids
  And the knowledge status remains unchanged

Scenario: promote rejects knowledge drawers as verification evidence
  Test:
    Filter: test_cli_knowledge_promote_rejects_knowledge_verification_ref
    Level: integration
  Given a candidate knowledge drawer exists
  And another knowledge drawer exists
  When running `mempal knowledge promote <id> --status promoted --verification-ref <knowledge_id> --reason bad`
  Then the command fails
  And the error says lifecycle refs must point to evidence drawers
  And the knowledge status remains unchanged

Scenario: demote rejects missing evidence refs
  Test:
    Filter: test_cli_knowledge_demote_rejects_missing_evidence_ref
    Level: integration
  Given a promoted knowledge drawer exists
  When running `mempal knowledge demote <id> --status demoted --evidence-ref drawer_missing --reason bad --reason-type contradicted`
  Then the command fails
  And the error says ref drawer not found
  And the knowledge status remains unchanged

Scenario: lifecycle still accepts real evidence refs
  Test:
    Filter: test_cli_knowledge_lifecycle_accepts_evidence_refs
    Level: integration
  Given a candidate knowledge drawer exists
  And an evidence drawer exists
  When running `mempal knowledge promote <id> --status promoted --verification-ref <evidence_id> --reason validated`
  Then the command succeeds
  And the knowledge status becomes promoted
  And `verification_refs` contains `<evidence_id>`
