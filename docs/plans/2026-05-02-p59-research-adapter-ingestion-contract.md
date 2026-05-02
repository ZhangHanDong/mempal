# P59 Research Adapter Ingestion Contract

## Goal

Add `mempal phase3 research-validate-plan` to validate external research report
JSON before any future evidence-first ingestion implementation.

## Verification

```bash
agent-spec parse specs/p59-research-adapter-ingestion-contract.spec.md
agent-spec lint specs/p59-research-adapter-ingestion-contract.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_research_validate_plan
cargo test --test phase3_runtime test_cli_phase3_research_validate_plan_reports_missing_fields
```
