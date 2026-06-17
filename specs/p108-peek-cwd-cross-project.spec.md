spec: task
name: "P108: peek cwd cross-project"
inherits: project
tags: [cowork, peek, cli, mcp]
estimate: 0.5d
---

## Intent

P108 lets an operator or agent read a partner coding agent's LIVE session for a
SPECIFIC project, not only the project the MCP server happens to run in. The
underlying `cowork::peek::peek_partner` already resolves sessions by
`PeekRequest.cwd`, but neither runtime entrypoint exposes that: the
`mempal_peek_partner` MCP handler hardcodes `std::env::current_dir()`, and there
is no shell-level peek command at all. P108 plumbs `cwd` through the MCP request
(optional, backward-compatible) and adds a `mempal cowork-peek` CLI command so
no-tmux, cross-project live reading works from either entrypoint.

## Decisions

- The MCP `PeekPartnerRequest` (`src/mcp/tools.rs`) gains an optional `cwd`
  field. When present and non-empty the `mempal_peek_partner` handler
  (`src/mcp/server.rs`) reads that project's partner session; when omitted it
  falls back to `std::env::current_dir()` exactly as before.
- Handler cwd resolution is extracted into a pure helper `resolve_peek_cwd`
  (`src/mcp/server.rs`) so the fallback logic is unit-testable.
- The `mempal_peek_partner` tool description notes that `cwd` reads a partner
  session in another project, defaulting to the server's project.
- A new CLI command `mempal cowork-peek --tool <claude|codex> --cwd <path>
  [--limit N] [--since RFC3339] [--format plain|json]` (`src/main.rs`) calls the
  existing `peek_partner` with `caller_tool: None`, dispatched in the
  cowork graceful-degrade block (no palace.db required), and printing a plain
  header + messages or the serialized `PeekResponse` as JSON.
- No change to `peek_partner`, session resolution, the self-peek / infer-partner
  rules, or any storage. The signal flow is read-only as today.

## Boundaries

### Allowed Changes
- src/mcp/tools.rs
- src/mcp/server.rs
- src/main.rs
- specs/p108-peek-cwd-cross-project.spec.md
- docs/plans/2026-06-17-p108-peek-cwd-cross-project.md
- CHANGELOG.md
- Cargo.toml
- AGENTS.md
- CLAUDE.md

### Forbidden
- Do not change `peek_partner` session-resolution, self-peek, or
  infer-partner logic.
- Do not make `cwd` required on the MCP request (must stay backward-compatible).
- Do not write to the database, inbox, events, or registry from peek.
- Do not add new tools, tables, schema versions, or feature flags.

## Acceptance Criteria

Scenario: MCP peek request resolves an explicit cwd and falls back when omitted
  Test:
    Filter: cargo test --lib mcp::server::tests::test_resolve_peek_cwd_honors_explicit_and_falls_back
  Given an explicit non-empty cwd string
  When the peek cwd is resolved
  Then the resolved path equals that cwd
  And resolving an omitted cwd yields the process current directory

Scenario: MCP peek tool schema documents the cwd field
  Test:
    Filter: cargo test --lib mcp::server::tests::test_mcp_peek_partner_schema_documents_cwd
  Given the `mempal_peek_partner` tool input schema
  When it is serialized
  Then it contains a `cwd` property
  And it mentions reading another project's session

Scenario: CLI exposes a cross-project cowork-peek command
  Test:
    Filter: cargo test --bin mempal test_cli_cowork_peek_parses
  Given the argument vector `mempal cowork-peek --tool codex --cwd /tmp/project`
  When the CLI parses it
  Then the parsed command is the cowork-peek variant
  And it carries tool "codex" and cwd "/tmp/project"

## Out of Scope

- Auto-discovering tmux panes or reading arbitrary terminal screens (that stays
  `cowork-tmux-peek`); P108 is session-log peek only.
- Changing how `peek_partner` encodes or scans session paths.
- A persistent or cross-project live "viewer" UI; P108 is a one-shot read.
- mtime-based active-session selection changes (separate future work).
