spec: task
name: "P74: card context default-on proposal"
inherits: project
tags: [phase-3, self-evolution, card-context, default-policy]
---

## Intent

P66 can report whether card-aware context is ready for a future default-on spec,
but it does not produce a complete proposal artifact. P74 adds a read-only
default-on proposal surface for card context that combines P66 readiness evidence
with explicit rollback criteria while preserving the current `include_cards`
opt-in runtime default.

## Decisions

- P74 must leave its own `specs/p74-*.spec.md` and plan document.
- Add CLI `mempal phase3 default-proposal card-context`.
- Add MCP `mempal_phase3 action=default_proposal` with `candidate=card-context`.
- The proposal reuses P66 `card_context_default_readiness`.
- The proposal requires at least one explicit rollback criterion before it can
  be marked `proposal_ready=true`.
- The proposal output must include `writes=false`, `candidate`,
  `proposal_ready`, `decision`, embedded readiness, rollback criteria, and
  reasons.
- P74 must not change `mempal context` or `mempal_context` default
  `include_cards=false`.

## Boundaries

### Allowed Changes
- specs/p74-card-context-default-proposal.spec.md
- docs/plans/2026-05-13-p74-card-context-default-proposal.md
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
- Do not change `mempal context` default `include_cards=false`.
- Do not change `mempal_context` default `include_cards=false`.
- Do not write runtime adoption events.
- Do not mutate knowledge cards or knowledge lifecycle state.
- Do not enable card embeddings.
- Do not mark the overall self-evolution objective complete.

## Acceptance Criteria

Scenario: CLI proposal is ready when readiness and rollback criteria are satisfied
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_proposal_card_context_ready
    Targets: CLI default proposal behavior in `src/main.rs`.
  Given three accepted `card_context/include_cards` adoption events
  And rollback criterion is `rollback on contradiction or user-visible degradation`
  When running `mempal phase3 default-proposal card-context --rollback-criterion "rollback on contradiction or user-visible degradation" --format json`
  Then stdout contains `writes=false`
  And stdout contains `proposal_ready=true`
  And stdout contains decision `eligible_for_default_on_spec`
  And no runtime adoption event is persisted by the proposal command
  And this verifies the `src/main.rs` CLI entry point

Scenario: CLI proposal blocks without rollback criteria
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_proposal_requires_rollback_criteria
    Targets: CLI rollback-criteria gate.
  Given readiness evidence is satisfied
  When running default proposal without `--rollback-criterion`
  Then stdout contains `proposal_ready=false`
  And reasons mention `rollback criteria are required`

Scenario: CLI proposal blocks when readiness is not satisfied
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_proposal_blocks_without_readiness
    Targets: CLI readiness gate.
  Given no `card_context/include_cards` adoption evidence
  When running default proposal with rollback criteria
  Then stdout contains `proposal_ready=false`
  And embedded readiness has `ready=false`

Scenario: CLI proposal does not change context default
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_proposal_keeps_context_cards_opt_in
    Targets: Runtime context default behavior.
  Given a promoted active card exists
  And a ready default proposal is generated
  And query is `card-aware`
  When running `mempal context "card-aware" --format json` without `--include-cards`
  Then the context output omits card items

Scenario: MCP proposal mirrors CLI read-only contract
  Test:
    Filter: cargo test mcp::server::tests::test_mcp_phase3_default_proposal_card_context_is_read_only --lib
    Targets: MCP default proposal action in `src/mcp/server.rs`.
  Given `mempal_phase3 action=default_proposal` and `candidate=card-context`
  When called with rollback criteria
  Then the response contains a default proposal report
  And no runtime adoption event is persisted by the action
  And this verifies the `src/mcp/server.rs` MCP entry point

Scenario: Default proposal rejects unknown candidate
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_default_proposal_rejects_unknown_candidate
    Targets: CLI default proposal input validation.
  Given an unsupported default proposal candidate
  When running default proposal
  Then the command fails
  And stderr mentions `unsupported phase3 default proposal candidate`

Scenario: Inventories include P74
  Test:
    Filter: rg -n "p74-card-context-default-proposal|P74 card context default-on proposal" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
    Targets: Agent inventories and design document.
  Given project documentation
  When searching for P74
  Then the spec, plan, and design status are recorded

## Out of Scope

- Actually changing `include_cards` defaults.
- Card embeddings.
- Automatic rollback executor.
- New database tables.
