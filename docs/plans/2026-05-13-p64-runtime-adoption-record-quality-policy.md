# P64 Runtime Adoption Record Quality Policy

Goal: add a read-only preflight quality check for candidate runtime adoption
records. The check should help agents avoid writing low-signal Phase-3 evidence,
without adding hooks, automatic writes, schema changes, or default runtime
policy changes.

Spec: `specs/p64-runtime-adoption-record-quality-policy.spec.md`

## Steps

- [x] Add shared core quality report types and policy logic.
- [x] Add CLI `mempal phase3 adoption check-record` with plain/json output.
- [x] Add MCP `mempal_phase3 action=check_record`.
- [x] Update protocol, MIND-MODEL design, AGENTS, and CLAUDE docs.
- [x] Verify targeted tests, full local checks, spec parse/lint, and diff check.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p64-runtime-adoption-record-quality-policy.spec.md
agent-spec lint specs/p64-runtime-adoption-record-quality-policy.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_json_accepts_supported_event
cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_json_warns_on_weak_evidence
cargo test --test phase3_runtime test_cli_phase3_adoption_check_record_rejects_empty_feature
cargo test mcp::server::tests::test_mcp_phase3_check_record_action_is_read_only --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
git diff --check
```
