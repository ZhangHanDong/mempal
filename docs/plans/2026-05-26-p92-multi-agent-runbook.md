# P92 Multi-Agent Cowork Runbook

Spec: `specs/p92-multi-agent-runbook.spec.md`

## Plan

- [x] Define P92 task contract and implementation plan.
- [x] Add `docs/COWORK-RUNBOOK.md`.
- [x] Add read-only CLI `cowork-runbook --format plain|json`.
- [x] Add CLI tests for plain, JSON, invalid format, and side effects.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p92-multi-agent-runbook.spec.md
agent-spec lint specs/p92-multi-agent-runbook.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_runbook_plain
cargo test --test cowork_bus test_cli_cowork_runbook_json
cargo test --test cowork_bus test_cli_cowork_runbook_rejects_invalid_format
rg -n "p92-multi-agent-runbook|P92 multi-agent cowork runbook|COWORK-RUNBOOK" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
```
