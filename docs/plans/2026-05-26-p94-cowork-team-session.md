# P94 Cowork Team Session

Spec: `specs/p94-cowork-team-session.spec.md`

## Plan

- [x] Define P94 task contract and implementation plan.
- [x] Add session runtime file model in cowork bus core.
- [x] Add CLI create/list/status commands.
- [x] Add MCP session actions.
- [x] Add CLI/MCP tests for create/list/status/error paths.
- [x] Update AGENTS / CLAUDE / MIND-MODEL-DESIGN inventories.
- [x] Run spec validation, targeted tests, and Rust verification.

## Verification

```bash
agent-spec parse specs/p94-cowork-team-session.spec.md
agent-spec lint specs/p94-cowork-team-session.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_session_create_and_list
cargo test --test cowork_bus test_cli_cowork_session_rejects_unknown_agent
cargo test --test cowork_bus test_cli_cowork_session_status_update
cargo test --lib test_mcp_cowork_bus_sessions
```
