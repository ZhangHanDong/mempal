# P109 Implementation Plan — cross-project resume

Spec: `specs/p109-cross-project-resume.spec.md`
Target release: 0.7.0 (new capability, MINOR)

## Context

mempal already stores every project's memory in one global `palace.db` keyed by
`wing`, and each project's absolute path is in its `worktree://` anchor_id. But
no entrypoint turns a fuzzy project name into "here is the project, where it
lives, and where we left off." P109 adds `projects` (list) and `resume`
(fuzzy-resolve → pack) on both CLI and MCP, read-only and embedder-free, so
"continue project xxx" is one resolve→load step from any directory. mempal
supplies the path + memory; the agent does the cd.

## Tasks

- [ ] T1. Spec (done first).
- [ ] T2. Plan (this file).
- [ ] T3. `src/projects.rs` — types (`ProjectSummary`, `ResumeEvidence`,
      `ResumeCandidate`, `ResumePack`, `ResumeResolution`) + `list_projects` +
      `resume_project`, querying via `Database::conn()`. Derive Serialize +
      `rmcp::schemars::JsonSchema` so MCP can return them directly.
- [ ] T4. `src/lib.rs` — `pub mod projects;`.
- [ ] T5. `src/main.rs` — `Projects` + `Resume` command variants; dispatch arms
      after DB open (next to `brief`); `projects_command` / `resume_command`
      plain+json handlers.
- [ ] T6. `src/mcp/tools.rs` — `ResumeRequest` input DTO (query + optional
      limits).
- [ ] T7. `src/mcp/server.rs` — `mempal_projects` (no args) + `mempal_resume`
      tools returning the projects types.
- [ ] T8. `src/core/protocol.rs` — short rule pointing at projects/resume for
      cross-project recall.
- [ ] T9. Tests:
      - `tests/projects_resume.rs`: list paths+counts, resume unique/ambiguous/
        not-found.
      - `src/main.rs`: `test_cli_projects_and_resume_parse`.
      - `src/mcp/server.rs`: `test_mcp_registry_includes_projects_and_resume`.
- [ ] T10. `cargo test` green; `cargo clippy` clean; `agent-spec lint` >= 0.7.
- [ ] T11. Version bump 0.7.0: `Cargo.toml` + `CHANGELOG.md`.
- [ ] T12. Inventory sync: AGENTS.md + CLAUDE.md spec/plan tables + MCP tool list
      (now 25 tools) + code-structure note for `src/projects.rs`.
- [ ] T13. Commit on main, `cargo publish`, tag v0.7.0.

## Verification

- Acceptance: the six test filters in the spec.
- Regression: full `cargo test`.
- Manual: from another project dir, `mempal projects` lists wings with paths;
  `mempal resume mempal` returns this repo's path + recent evidence + next step.

## Rollback

Additive. Revert the commit to remove the module, commands, and tools; no schema
or data change.
