spec: task
name: "P73: evaluator advisory API"
inherits: project
tags: [phase-3, evaluator, self-evolution, advisory]
---

## Intent

P50 fixed evaluator boundaries as advisory-only, and P58 added a read-only gate
for deciding whether evaluator APIs are mature enough to implement. P73 adds the
first evaluator advice surface with replayable deterministic output: agents can
ask for an advisory lifecycle recommendation and receive reasons plus a runtime
adoption capture plan, but no lifecycle state is mutated.

## Decisions

- P73 must leave its own `specs/p73-*.spec.md` and plan document.
- Add CLI `mempal phase3 evaluator advise`.
- Add MCP `mempal_phase3 action=evaluator_advise`.
- Advice output must be deterministic from request fields and existing policy,
  with no LLM calls, network calls, or hidden runtime state.
- Advice output must include `writes=false`, `lifecycle_authority=false`,
  `deterministic_gate_required=true`, recommendation, reasons, and an adoption
  capture plan for `surface=evaluator`.
- `dao_tian` canonicalization advice must always require human review.
- Evidence-free advice must recommend more evidence instead of promotion.
- Counterexamples or risk notes must block supportive recommendation.

## Boundaries

### Allowed Changes
- specs/p73-evaluator-advisory-api.spec.md
- docs/plans/2026-05-13-p73-evaluator-advisory-api.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/core/phase3.rs
- src/main.rs
- src/mcp/server.rs
- src/mcp/tools.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add schema migrations.
- Do not write runtime adoption events from evaluator advice.
- Do not mutate knowledge drawers, knowledge cards, lifecycle refs, or audit
  events.
- Do not satisfy reviewer requirements through evaluator output.
- Do not bypass deterministic promotion/card gates.
- Do not add evaluator scoring, LLM calls, or network calls.
- Do not mark the overall self-evolution objective complete.

## Acceptance Criteria

Scenario: CLI evaluator advice is replayable and read-only
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_supportive_read_only
    Targets: CLI evaluator advice behavior in `src/main.rs`.
  Given evaluator `eval_policy` advises on a `dao_ren` promotion with two evidence refs
  When running `mempal phase3 evaluator advise --evaluator-id eval_policy --subject-kind dao_ren --subject-id k1 --proposed-action promote --evidence-ref e1 --evidence-ref e2 --format json`
  Then stdout contains `writes=false` and `lifecycle_authority=false`
  And stdout contains `deterministic_gate_required=true`
  And stdout recommends `advisory_support`
  And no runtime adoption event is persisted
  And this verifies the `src/main.rs` CLI entry point

Scenario: CLI evaluator advice requires human review for dao_tian canonicalization
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_dao_tian_requires_human_review
    Targets: CLI evaluator human-review policy.
  Given evaluator advice for `dao_tian` `canonical`
  When running evaluator advice with supporting evidence
  Then stdout contains `requires_human_review=true`
  And reasons mention `dao_tian canonicalization requires human review`

Scenario: CLI evaluator advice rejects weak or risky recommendations
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_needs_evidence_and_blocks_risk
    Targets: CLI evaluator evidence/risk policy.
  Given evaluator advice without evidence refs
  When running evaluator advice
  Then recommendation is `needs_evidence`
  Given evaluator advice with a risk note or counterexample ref
  When running evaluator advice
  Then recommendation is `do_not_promote`

Scenario: MCP evaluator advice mirrors CLI read-only contract
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_evaluator_advise_action_is_read_only --lib
    Targets: MCP evaluator advice action in `src/mcp/server.rs`.
  Given `mempal_phase3 action=evaluator_advise`
  When called with evaluator id, subject, proposed action, and evidence refs
  Then the response contains the same advisory fields
  And no runtime adoption event is persisted
  And this verifies the `src/mcp/server.rs` MCP entry point

Scenario: Evaluator advice validates required fields
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_evaluator_advise_rejects_missing_evaluator
    Targets: CLI evaluator input validation in `src/main.rs`.
  Given a request without evaluator id
  When running evaluator advice
  Then the command fails
  And stderr mentions `evaluator-id is required`

Scenario: Inventories include P73
  Test:
    Filter: rg -n "p73-evaluator-advisory-api|P73 evaluator advisory API" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Agent inventories and design document.
  Given project documentation
  When searching for P73
  Then the spec, plan, and design status are recorded

## Out of Scope

- Evaluator-driven promotion or demotion.
- Automatic evaluator invocation.
- Evaluator scoring or model integration.
- New persistence tables.
- Default-on card context.
