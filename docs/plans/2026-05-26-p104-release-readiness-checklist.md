# P104 Release Readiness Checklist

Spec: `specs/p104-release-readiness-checklist.spec.md`

## Tasks

- [x] Define the P104 task contract and implementation plan.
- [x] Add CLI `release-readiness --format plain|json`.
- [x] Add deterministic checklist for metadata, docs, specs/plans, doctor, and schema.
- [x] Add integration tests for JSON, plain, and invalid format.
- [x] Update AGENTS / CLAUDE / MIND-MODEL inventory through P104.

## Verification

```bash
agent-spec parse specs/p104-release-readiness-checklist.spec.md
agent-spec lint specs/p104-release-readiness-checklist.spec.md --min-score 0.7
cargo test --test ops_runtime test_cli_release_readiness_json
cargo test --test ops_runtime test_cli_release_readiness_plain
cargo test --test ops_runtime test_cli_release_readiness_rejects_invalid_format
```
