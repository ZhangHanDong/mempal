# P62 Runtime Adoption CLI Guidance

## Goal

Expose the P61 runtime adoption recording protocol through CLI parity:
`mempal phase3 adoption guidance`.

## Scope

- Add `specs/p62-runtime-adoption-cli-guidance.spec.md`.
- Add `mempal phase3 adoption guidance --format plain|json`.
- Move runtime adoption guidance into shared core code used by MCP and CLI.
- Update `MIND-MODEL-DESIGN.md`, `AGENTS.md`, and `CLAUDE.md`.
- Preserve read-only behavior and avoid schema/runtime default changes.

## Steps

- [x] Write failing CLI guidance tests.
- [x] Implement shared guidance data and CLI command.
- [x] Keep MCP guidance on the shared implementation.
- [x] Update design and repository inventories.
- [x] Run spec checks and Rust verification.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p62-runtime-adoption-cli-guidance.spec.md
agent-spec lint specs/p62-runtime-adoption-cli-guidance.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_json_is_read_only
cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_plain
cargo test --test phase3_runtime test_cli_phase3_adoption_guidance_rejects_invalid_format
cargo test mcp::server::tests::test_mcp_phase3_guidance_action_is_read_only --lib
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
git diff --check
```
