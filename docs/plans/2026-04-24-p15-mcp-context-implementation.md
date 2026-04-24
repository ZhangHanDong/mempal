# P15 MCP Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the P14 mind-model context assembler to MCP clients as `mempal_context`.

**Architecture:** Keep `src/context.rs` as the single assembly source of truth. Add MCP DTOs in `src/mcp/tools.rs`, a thin handler in `src/mcp/server.rs`, and protocol/docs updates so agents discover the new tool.

**Tech Stack:** Rust 2024, rmcp tool macros, serde/schemars DTOs, existing embedder/search/database stack.

---

## Source Spec

- `specs/p15-mcp-context.spec.md`

## Tasks

- [x] Add `ContextRequest` / `ContextResponse` DTOs in `src/mcp/tools.rs`.
- [x] Add `mempal_context` handler in `src/mcp/server.rs`.
- [x] Avoid holding `Database` across `.await` by using `assemble_context_with_vector`.
- [x] Add MCP handler tests for tier ordering, defaults, evidence opt-in, invalid params, no DB side effects, and tool registry.
- [x] Update `MEMORY_PROTOCOL`, usage docs, AGENTS.md, CLAUDE.md, and mind-model design notes.
- [x] Run final verification and commit.

## Verification

```bash
agent-spec parse specs/p15-mcp-context.spec.md
agent-spec lint specs/p15-mcp-context.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo check --features rest
```
