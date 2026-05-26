# P98 Release Install Doctor

Spec: `specs/p98-release-install-doctor.spec.md`

## Tasks

- [x] Define the P98 task contract and implementation plan.
- [x] Add read-only doctor core model for binary, PATH, and DB schema checks.
- [x] Add CLI `doctor --format plain|json` before normal DB open.
- [x] Add deterministic integration tests for schema/path, missing DB, and bad format.
- [x] Update AGENTS / CLAUDE / MIND-MODEL inventory.

## Verification

```bash
agent-spec parse specs/p98-release-install-doctor.spec.md
agent-spec lint specs/p98-release-install-doctor.spec.md --min-score 0.7
cargo test --test ops_runtime test_cli_doctor_json_reports_schema_and_path
cargo test --test ops_runtime test_cli_doctor_plain_no_db_is_read_only
cargo test --test ops_runtime test_cli_doctor_rejects_invalid_format
```
