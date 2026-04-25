# P22 MCP Knowledge Distill Implementation Plan

**Goal:** Expose P18 deterministic knowledge distill through MCP as `mempal_knowledge_distill`, so agents can create candidate knowledge drawers from evidence refs without shelling out.

**Architecture:** Extract distill preparation and commit into a shared library module. Keep database work synchronous and separated from async embedding so MCP futures remain `Send`.

## Task 1: Contract And Shared Distill Core

- [x] Add `specs/p22-mcp-knowledge-distill.spec.md`.
- [x] Add `src/knowledge_distill.rs` with `prepare_distill` and `commit_distill`.
- [x] Update CLI `mempal knowledge distill` to reuse the shared core.
- [x] Preserve P18 behavior: dry-run deterministic, existing drawer idempotent, no LLM, no auto-promotion.

## Task 2: MCP Tool Surface

- [x] Add `KnowledgeDistillRequest` / `KnowledgeDistillResponse`.
- [x] Add `mempal_knowledge_distill` MCP handler.
- [x] Avoid holding `Database` across `.await` in the MCP handler.
- [x] Add MCP tests for create, dry-run, bad tier, bad refs, trigger hints, idempotency, registry, and protocol exposure.

## Task 3: Docs And Verification

- [x] Update `MEMORY_PROTOCOL`, usage docs, MIND-MODEL implemented surface, and repo status docs.
- [x] Run spec parse/lint.
- [ ] Run full verification, commit, ingest project memory, push branch, and open PR.

## Verification Commands

```bash
agent-spec parse specs/p22-mcp-knowledge-distill.spec.md
agent-spec lint specs/p22-mcp-knowledge-distill.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test mcp::server::tests::test_mcp_knowledge_distill -- --nocapture
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_mempal_knowledge_distill -- --nocapture
cargo test --test knowledge_lifecycle test_cli_knowledge_distill -- --nocapture
cargo test
```
