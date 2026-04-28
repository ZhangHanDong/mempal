# P45 Card Linked-Evidence Retrieval Plan

**Goal:** Add an explicit Phase-2 card retrieval surface that finds active
knowledge cards through matched evidence drawers, without changing default
search or adding card embeddings.

## Checklist

- [x] Add P45 task contract.
- [x] Add core retrieval model and linked-evidence search implementation.
- [x] Add CLI `knowledge-card retrieve`.
- [x] Add MCP `mempal_knowledge_cards` action `retrieve`.
- [x] Update MIND-MODEL and inventory docs.
- [x] Run spec lint, formatting, checks, clippy, and tests.

## Verification Commands

```bash
agent-spec parse specs/p45-card-linked-evidence-retrieval.spec.md
agent-spec lint specs/p45-card-linked-evidence-retrieval.spec.md --min-score 0.7
cargo fmt
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test --test knowledge_card_retrieval
cargo test test_mcp_knowledge_cards_retrieve_action
cargo test
```
