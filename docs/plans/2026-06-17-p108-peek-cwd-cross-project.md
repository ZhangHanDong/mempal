# P108 Implementation Plan — peek cwd cross-project

Spec: `specs/p108-peek-cwd-cross-project.spec.md`
Target release: 0.6.2 (capability completion, backward-compatible)

## Context

Codex (live, in the mempal project) surfaced that `mempal_peek_partner` is
cwd-locked to the MCP server's own process directory: the handler hardcodes
`std::env::current_dir()` and the MCP request has no `cwd`, so you cannot peek a
partner agent working in another project. There is also no shell-level peek
command for no-tmux, cross-project live reading. The cowork layer already
supports it — `cowork::peek::PeekRequest` has a `cwd` field honored by
`peek_partner` — it just was never plumbed to either entrypoint. P108 exposes it.

## Tasks

- [ ] T1. Spec (done first).
- [ ] T2. Plan (this file).
- [ ] T3. `src/mcp/tools.rs` — add optional `cwd: Option<String>` to
      `PeekPartnerRequest` with a doc comment (drives the MCP schema).
- [ ] T4. `src/mcp/server.rs` — add `resolve_peek_cwd(Option<String>)` helper;
      use it in `mempal_peek_partner`; extend the tool description to mention cwd.
- [ ] T5. `src/main.rs` — add `CoworkPeek { tool, cwd, limit, since, format }`
      command variant; dispatch arm in the cowork graceful-degrade block; add it
      to the no-DB skip list; implement `cowork_peek_command` calling
      `peek_partner` (caller_tool None) with plain + json output.
- [ ] T6. Tests:
      - `src/mcp/server.rs`: `test_resolve_peek_cwd_honors_explicit_and_falls_back`,
        `test_mcp_peek_partner_schema_documents_cwd`.
      - `src/main.rs`: new `#[cfg(test)] mod tests` with
        `test_cli_cowork_peek_parses`.
- [ ] T7. `cargo test` green; `cargo clippy` clean; `agent-spec lint` >= 0.7.
- [ ] T8. Version bump 0.6.2: `Cargo.toml` + `CHANGELOG.md`.
- [ ] T9. Inventory sync: AGENTS.md + CLAUDE.md spec/plan tables + cowork CLI note.
- [ ] T10. Commit on main, `cargo publish`, tag v0.6.2.

## Verification

- Acceptance: the three test filters in the spec.
- Regression: full `cargo test` (no change to peek_partner behavior).
- Manual: `mempal cowork-peek --tool codex --cwd <other-project>` prints that
  project's partner session; `mempal_peek_partner` with no cwd behaves as before.

## Rollback

Additive + backward-compatible. Revert the commit to remove the CLI command and
the optional MCP field; no data migration, no schema change.
