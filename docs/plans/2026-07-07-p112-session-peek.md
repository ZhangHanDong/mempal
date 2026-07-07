# P112 Explicit MCP Session Peek Plan

## Context

P108 added cross-project `cwd` support to `mempal_peek_partner` and the CLI
`mempal cowork-peek`. That solved cross-project partner reads, but it preserved
the partner-only self-peek guard. The observed failure is therefore expected:
a Codex MCP caller asking `mempal_peek_partner(tool="codex", cwd=...)` is
rejected before the transcript reader runs.

The CLI proves the lower-level reader already handles the target use case:
`mempal cowork-peek --tool codex --cwd /Users/zhangalex/Work/Projects/consult/agentsview`
finds the local Codex session transcript. P112 should expose that explicit
session-read behavior to MCP without changing the semantics of the partner
tool.

## Implementation Steps

1. Keep the contract and inventory current.
   - Add `specs/p112-session-peek.spec.md`.
   - Add this plan.
   - Update `AGENTS.md` and `CLAUDE.md` to list P112 as current draft and this
     plan as draft/unimplemented.

2. Add RED tests first.
   - Same-tool Codex cross-project session read succeeds through the new MCP
     surface.
   - Legacy `mempal_peek_partner` still rejects self-peek from a same-tool MCP
     caller.
   - `tool="auto"`, unknown tool names, and missing or empty `cwd` fail before
     transcript discovery.
   - Successful reads produce no `palace.db`, cowork inbox, cowork event log,
     session registry, or adoption-event writes.
   - The tool registry/protocol guidance expose the new surface.
   - JSON response uses `tool`, not `partner_tool`.

3. Add MCP DTOs in `src/mcp/tools.rs`.
   - Define `SessionPeekRequest` with required `tool` and `cwd`, plus optional
     `limit`.
   - Define a session-oriented response shape mirroring existing peek message
     fields while avoiding partner naming.
   - Register `mempal_session_peek` in the MCP tool list with schema docs that
     reject `auto` by contract.

4. Add the MCP handler in `src/mcp/server.rs`.
   - Parse `tool` as a concrete `Tool`.
   - Validate that `cwd` is present and non-empty.
   - Call the existing cowork peek/session reader path with no `caller_tool`
     self-peek context.
   - Map missing transcript and unsupported-tool errors into clear MCP errors.

5. Keep parser logic shared.
   - If the existing public function shape is too partner-specific, add a small
     wrapper in `src/cowork/peek.rs` that takes explicit `tool + cwd` and calls
     the same transcript discovery/extraction logic.
   - Do not change Codex or Claude transcript parser behavior in P112.

6. Update protocol and usage docs.
   - `src/core/protocol.rs`: explain that `mempal_session_peek` is the explicit
     same-tool/cross-project read surface.
   - Usage docs: distinguish partner peek, explicit session peek, and bus
     `tmux_peek`.

7. Verify.
   - `agent-spec parse specs/p112-session-peek.spec.md`
   - `agent-spec lint specs/p112-session-peek.spec.md --min-score 0.7`
   - `cargo fmt -- --check`
   - `cargo check`
   - `cargo clippy -- -D warnings`
   - `cargo test`

## Non-Goals

- No schema migration.
- No streaming transcript tail.
- No new write/control channel into Codex or Claude sessions.
- No relaxation of `mempal_peek_partner` self-peek protection.
