# P21 MCP Knowledge Gate Implementation Plan

**Goal:** Expose the P20 read-only knowledge promotion gate through MCP as `mempal_knowledge_gate`, so agents can evaluate promotion readiness without shelling out.

**Architecture:** Extract the gate evaluator into a shared library module and make both CLI and MCP call the same code path. Keep the tool read-only and schema-free.

## Task 1: Contract And Shared Evaluator

- [x] Add `specs/p21-mcp-knowledge-gate.spec.md`.
- [x] Extract P20 gate report and evaluator into `src/knowledge_gate.rs`.
- [x] Keep CLI `mempal knowledge gate` output compatible by reusing the shared evaluator.

## Task 2: MCP Tool Surface

- [x] Add `KnowledgeGateRequest` / `KnowledgeGateResponse` DTOs.
- [x] Add `mempal_knowledge_gate` MCP handler.
- [x] Map invalid drawer/status/ref errors to MCP invalid-params errors.
- [x] Add MCP tests for allow, denial, reviewer, counterexamples, bad target, bad refs, and registry/protocol exposure.

## Task 3: Docs And Verification

- [x] Update `MEMORY_PROTOCOL`, usage docs, MIND-MODEL implemented surface, and repo status docs.
- [x] Run spec parse/lint.
- [ ] Run full verification, commit, ingest project memory, push branch, and open PR.

## Verification Commands

```bash
agent-spec parse specs/p21-mcp-knowledge-gate.spec.md
agent-spec lint specs/p21-mcp-knowledge-gate.spec.md --min-score 0.7
cargo fmt -- --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings
cargo test mcp::server::tests::test_mcp_knowledge_gate -- --nocapture
cargo test mcp::server::tests::test_mcp_tool_registry_and_protocol_include_mempal_knowledge_gate -- --nocapture
cargo test --test knowledge_lifecycle test_cli_knowledge_gate -- --nocapture
cargo test
cargo check --features rest
```
