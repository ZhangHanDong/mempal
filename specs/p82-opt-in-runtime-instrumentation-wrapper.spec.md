spec: task
name: "P82: opt-in runtime instrumentation wrapper"
inherits: project
tags: [phase-3, runtime-adoption, instrumentation, wrapper, cli]
---

## Intent

P77 allowed `opt_in_wrapper` as the only semi-automatic instrumentation mode,
but it intentionally stopped before implementing a wrapper. P82 adds the first
explicit runtime wrapper: a user-invoked CLI command that runs one child command,
maps the observed exit status into an adoption outcome, and routes any write
through the existing checked capture path. This closes the smallest practical
live instrumentation gap without adding hooks, daemons, silent background
capture, or autonomous lifecycle authority.

## Decisions

- P82 must leave its own `specs/p82-*.spec.md` and matching plan document.
- Add CLI `mempal phase3 adoption wrap`.
- The wrapper is explicit opt-in: it only runs when the user invokes the wrap
  command around a child command.
- The wrapper executes exactly one child command after `--`.
- Default outcome is `auto`: child exit code `0` maps to `accepted`, and
  non-zero exit maps to `rejected`.
- A caller may override the observed outcome with `--outcome`.
- Runtime adoption writes only happen when `--execute` is supplied.
- Any write must reuse the existing P72 capture mapping and P69 checked-record
  quality gate.
- Warning-quality writes remain blocked unless `--allow-warnings` is supplied.
- The wrapper must report child exit code, mapped outcome, capture report, and
  whether evidence was written.
- The wrapper must not install hooks, spawn background workers, or change
  runtime defaults.

## Boundaries

### Allowed Changes
- specs/p82-opt-in-runtime-instrumentation-wrapper.spec.md
- docs/plans/2026-05-25-p82-opt-in-runtime-instrumentation-wrapper.md
- docs/MIND-MODEL-DESIGN.md
- AGENTS.md
- CLAUDE.md
- src/main.rs
- src/core/protocol.rs
- tests/phase3_runtime.rs

### Forbidden
- Do not add schema migrations.
- Do not add MCP write access for shell command execution.
- Do not install shell hooks, daemon processes, or background capture.
- Do not bypass `capture` / `record_checked` quality gates.
- Do not write events unless `--execute` is explicitly supplied.
- Do not change existing `capture`, `record_checked`, gate, context, search, or
  default-control semantics.
- Do not grant autonomous promote/demote authority.

## Acceptance Criteria

Scenario: CLI wrap dry-run executes child but does not write evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_dry_run_executes_child_without_writing
    Targets: CLI wrapper dry-run behavior and DB side effects.
  Given an empty runtime adoption event table
  When running `mempal phase3 adoption wrap --surface runtime-context --query "context pack" --format json -- sh -c "exit 0"`
  Then the child command is executed
  And stdout is valid wrapper JSON with `writes=false`
  And `child_exit_code=0`
  And `outcome=accepted`
  And no runtime adoption event is persisted

Scenario: CLI wrap execute writes ready accepted evidence
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_execute_writes_ready_event
    Targets: CLI wrapper checked capture write path.
  Given an empty runtime adoption event table
  When running wrap with `--execute --surface runtime-context --query "context pack" --note "wrapper helped" --format json -- sh -c "exit 0"`
  Then the report has `writes=true`
  And the nested checked-record report has `blocked=false`
  And exactly one runtime adoption event is persisted with `signal=accepted`

Scenario: CLI wrap maps child failure to rejected and propagates failure
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_failure_maps_rejected_and_exits_nonzero
    Targets: CLI wrapper failure mapping and process exit behavior.
  Given an empty runtime adoption event table
  When running wrap with `--surface runtime-context --format json -- sh -c "exit 7"`
  Then stdout is valid wrapper JSON with `child_exit_code=7`
  And `outcome=rejected`
  And the wrapper exits with code `7`
  And no runtime adoption event is persisted

Scenario: CLI wrap blocks warning-quality writes by default
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_blocks_warning_by_default
    Targets: CLI wrapper checked quality gate.
  Given an empty runtime adoption event table
  When running wrap with `--execute --surface card-context --format json -- sh -c "exit 0"` without `--card-id`
  Then stdout is valid wrapper JSON with `writes=false`
  And the nested checked-record report has `blocked=true`
  And no runtime adoption event is persisted

Scenario: CLI wrap rejects missing child command
  Test:
    Filter: cargo test --test phase3_runtime test_cli_phase3_adoption_wrap_rejects_missing_child_command
    Targets: CLI wrapper input validation.
  Given the wrapper command without a child command
  When running `mempal phase3 adoption wrap --surface runtime-context`
  Then the command fails
  And stderr mentions that a child command is required

Scenario: Protocol and inventories include P82
  Test:
    Filter: rg -n "p82-opt-in-runtime-instrumentation-wrapper|P82 opt-in runtime instrumentation wrapper|phase3 adoption wrap" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
    Targets: Project inventories, design document, and embedded protocol.
  Given project documentation and protocol instructions
  When searching for P82 and the wrapper command
  Then the P82 spec, plan, design summary, and protocol guidance are recorded

## Out of Scope

- MCP-side command execution.
- Automatic live agent tool wrapping.
- Hook installation.
- Background adoption capture.
- Changing card context defaults.
- Automatic knowledge promotion or demotion.
