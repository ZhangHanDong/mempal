# P67 Research Adapter Evidence Ingest

Goal: add an explicit evidence-first CLI bridge from validated external research
reports into mempal evidence drawers without creating knowledge or bypassing
lifecycle gates.

Spec: `specs/p67-research-adapter-ingest.spec.md`

## Steps

- [x] Add P67 task contract and plan.
- [x] Add RED CLI tests for dry-run, execute/idempotency, invalid input, and format rejection.
- [x] Implement shared research ingest planning in `src/main.rs`.
- [x] Implement `mempal phase3 research-ingest-plan` dry-run and `--execute` paths.
- [x] Update MIND-MODEL, AGENTS, and CLAUDE docs.
- [x] Verify targeted tests, full local checks, spec parse/lint, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p67-research-adapter-ingest.spec.md
agent-spec lint specs/p67-research-adapter-ingest.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_dry_run_json_no_write
cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_execute_writes_research_evidence
cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_invalid_report_no_write
cargo test --test phase3_runtime test_cli_phase3_research_ingest_plan_rejects_invalid_format
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features rest -- -D warnings
cargo test
git diff --check
```
