spec: task
name: "P112: explicit MCP session peek"
inherits: project
tags: [mcp, cowork, live-session, cross-project, codex]
estimate: 1d
---

## Intent

P112 fixes a live-session access mismatch found after P108. The lower-level
cowork peek reader can read a Codex session for another project, but the MCP
`mempal_peek_partner` tool rejects the same request from a Codex caller as
self-peek because that tool is partner-only. Add an explicit read-only MCP
session peek surface for concrete `tool + cwd` reads without weakening the
existing partner self-peek protection.

## Decisions

- Add a new MCP tool named `mempal_session_peek`; do not relax or rename the
  existing `mempal_peek_partner` self-peek guard.
- `mempal_session_peek` requires a concrete `tool` value of `claude` or
  `codex` and a non-empty `cwd` project directory. It rejects `auto`, missing
  `cwd`, empty `cwd`, and unknown tool names.
- `mempal_session_peek` does not infer a caller tool from MCP `ClientInfo` and
  does not apply partner/self-peek semantics; it is an explicit session reader,
  equivalent in authorization shape to the existing shell `cowork-peek` command.
- Reuse the existing local transcript discovery and message extraction logic
  used by `cowork-peek`; do not duplicate Codex or Claude transcript parsers.
- The response uses session terminology: `tool`, `session_path`,
  `session_mtime`, `active`, `messages`, and `truncated`. It must not expose
  `partner_tool` or otherwise label a same-tool read as a partner read.
- `mempal_session_peek` returns data through the MCP response body only. It
  does not add `format`, `output`, `output_path`, or other file-output request
  fields.
- The MCP tool registry and memory protocol guidance document three distinct
  read-only live surfaces: `mempal_peek_partner` for Claude/Codex partner
  reads, `mempal_session_peek` for explicit same-tool or cross-project session
  reads, and cowork bus `tmux_peek` for registered concrete agent panes.
- P112 is read-only. It must not write `palace.db`, cowork inbox files, cowork
  event logs, session registries, or runtime adoption evidence.

## Boundaries

### Allowed Changes
- src/mcp/tools.rs
- src/mcp/server.rs
- src/cowork/peek.rs
- src/core/protocol.rs
- tests/cowork_peek.rs
- README.md
- README_zh.md
- docs/usage.md
- specs/p112-session-peek.spec.md
- docs/plans/2026-07-07-p112-session-peek.md
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not change the legacy `mempal_peek_partner` rule that rejects naming the
  caller's own tool.
- Do not change Codex or Claude transcript normalization, noise stripping, or
  message extraction semantics.
- Do not add a schema migration, database table, database column, or embedder
  dependency.
- Do not add a streaming tail, write-to-session, prompt injection, tmux send, or
  browser UI surface.
- Do not change cowork bus agent registration, inbox delivery, tmux transport,
  or delivery ack behavior.

## Acceptance Criteria

Rule: mcp-session-peek  Explicit MCP session peek is not partner peek

Scenario: mempal_session_peek reads a same-tool Codex session for another cwd
  Test:
    Filter: cargo test --lib test_mcp_session_peek_allows_same_tool_codex_cross_project
  Level: integration
  Test Double: fake transcript home
  Given a fake Codex transcript under the test home directory
  And the transcript records cwd "/tmp/agentsview"
  And the MCP caller client name is "codex-mcp-client"
  When `mempal_session_peek` is called with `tool` "codex", `cwd` "/tmp/agentsview", and `limit` 5
  Then the call succeeds without a self-peek error
  And the response `tool` is "codex"
  And the response contains the selected `session_path`, `session_mtime`, and transcript `messages`

Scenario: mempal_peek_partner still rejects MCP self-peek
  Test:
    Filter: cargo test --lib test_mcp_peek_partner_still_rejects_self_peek
  Given the MCP caller client name is "codex-mcp-client"
  And the requested partner `tool` is "codex"
  When `mempal_peek_partner` is called
  Then the call fails with the existing self-peek error
  And the failure text still explains that a caller cannot peek its own session

Scenario: mempal_session_peek rejects auto tool selection
  Test:
    Filter: cargo test --lib test_mcp_session_peek_rejects_auto_tool
  Given an MCP request for `mempal_session_peek`
  And the requested `tool` is "auto"
  And the requested `cwd` is "/tmp/agentsview"
  When the request is handled
  Then the call fails before transcript discovery
  And the error asks for a concrete `tool` value of "claude" or "codex"

Scenario: mempal_session_peek rejects unknown tool names
  Test:
    Filter: cargo test --lib test_mcp_session_peek_rejects_unknown_tool
  Given an MCP request for `mempal_session_peek`
  And the requested `tool` is "other-agent"
  And the requested `cwd` is "/tmp/agentsview"
  When the request is handled
  Then the call fails before transcript discovery
  And the error mentions the unsupported tool name

Scenario: mempal_session_peek requires cwd
  Test:
    Filter: cargo test --lib test_mcp_session_peek_requires_non_empty_cwd
  Given an MCP request for `mempal_session_peek`
  And the requested `tool` is "codex"
  When `cwd` is missing or empty
  Then the call fails before transcript discovery
  And the error explains that `cwd` is required for explicit session peek

Scenario: mempal_session_peek has no mempal or cowork side effects
  Test:
    Filter: cargo test --lib test_mcp_session_peek_has_no_mempal_or_cowork_side_effects
  Given a temporary mempal home with no cowork event log, inbox delivery, or palace database write for this request
  When `mempal_session_peek` reads an existing transcript
  Then no `palace.db` row is inserted or updated
  And no cowork inbox, cowork event log, session registry, or adoption event file is created or modified

Scenario: registry and protocol guidance advertise the distinct session peek surface
  Test:
    Filter: cargo test --lib test_mcp_tool_registry_and_protocol_include_session_peek
  Level: unit
  Targets: MCP tool registry and MEMORY_PROTOCOL text
  Given the MCP tool registry and memory protocol text
  When an agent inspects available live-session read surfaces
  Then `mempal_session_peek` is listed with required `tool` and `cwd` fields
  And the guidance says `mempal_peek_partner` remains for Claude/Codex partner reads
  And the guidance says cowork bus `tmux_peek` remains for registered concrete agent panes

Scenario: mempal_session_peek response does not use partner wording
  Test:
    Filter: cargo test --lib test_mcp_session_peek_response_uses_tool_not_partner_tool
  Given a successful `mempal_session_peek` response
  When the response is serialized to JSON
  Then it contains `tool`
  And it does not contain `partner_tool`

Scenario: mempal_session_peek has no file-output mode
  Test:
    Filter: cargo test --lib test_mcp_session_peek_has_no_file_output_mode
  Level: unit
  Targets: MCP request schema
  Given the `mempal_session_peek` request schema
  When an agent inspects output-related request fields
  Then the schema contains no `format`, `output`, or `output_path` field
  And successful calls return session data through the MCP response body

## Out of Scope

- Replacing or deprecating `mempal_peek_partner`.
- Choosing among multiple same-tool sessions for the same `cwd` beyond the
  existing latest-session selection logic.
- Real-time streaming, long-polling, or file tailing of transcripts.
- Reading hidden model/tool internals that current transcript parsers do not
  expose.
- Adding concrete `agent_id` routing to local Codex or Claude transcript logs.
