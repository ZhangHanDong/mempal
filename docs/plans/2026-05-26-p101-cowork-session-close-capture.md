# P101 Cowork Session Close Capture

Spec: `specs/p101-cowork-session-close-capture.spec.md`

## Tasks

- [x] Define the P101 task contract and implementation plan.
- [x] Add CLI `cowork-session-close`.
- [x] Add MCP action `session_close`.
- [x] Reuse P96 handoff capture for optional evidence write.
- [x] Add CLI/MCP tests and update docs inventory.

## Verification

```bash
agent-spec parse specs/p101-cowork-session-close-capture.spec.md
agent-spec lint specs/p101-cowork-session-close-capture.spec.md --min-score 0.7
cargo test --test cowork_bus test_cli_cowork_session_close_no_capture
cargo test --test cowork_bus test_cli_cowork_session_close_capture_execute
cargo test --lib test_mcp_cowork_bus_session_close
```
