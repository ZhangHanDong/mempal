# P83 Cognitive Brief

Goal: add a deterministic, citation-first `mempal brief` CLI report that
organizes existing context into a cognitive brief without LLM synthesis or DB
mutation.

Spec: `specs/p83-cognitive-brief.spec.md`

## Plan

- [x] Add P83 task contract and implementation plan.
- [x] Add failing CLI tests for JSON brief, plain brief, empty uncertainty, and
      invalid format.
- [x] Implement `src/brief.rs` using existing context assembly with evidence and
      cards enabled.
- [x] Wire top-level CLI `mempal brief <query>` with plain and JSON output.
- [x] Update MEMORY_PROTOCOL, MIND-MODEL, AGENTS, and CLAUDE inventories.
- [x] Run spec validation, targeted tests, and full Rust verification.

## Verification

```bash
agent-spec parse specs/p83-cognitive-brief.spec.md
agent-spec lint specs/p83-cognitive-brief.spec.md --min-score 0.7
cargo test --test cognitive_brief test_cli_brief_json_includes_citations_uncertainty_and_actions
cargo test --test cognitive_brief test_cli_brief_plain_lists_sections_and_citations
cargo test --test cognitive_brief test_cli_brief_no_evidence_reports_uncertainty
cargo test --test cognitive_brief test_cli_brief_rejects_invalid_format
rg -n "p83-cognitive-brief|P83 cognitive brief|mempal brief" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md src/core/protocol.rs
cargo fmt -- --check
cargo check
cargo clippy -- -D warnings
cargo test
cargo check --features rest
cargo clippy --features rest -- -D warnings
cargo test --features rest
git diff --check
```
