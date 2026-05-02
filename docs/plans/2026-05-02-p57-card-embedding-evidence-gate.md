# P57 Card Embedding Evidence Gate

## Goal

Add a read-only `mempal phase3 gate card-embeddings` readiness check that
requires measured miss evidence before card embeddings can be implemented.

## Verification

```bash
agent-spec parse specs/p57-card-embedding-evidence-gate.spec.md
agent-spec lint specs/p57-card-embedding-evidence-gate.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_gate_blocks_card_embeddings_without_miss_evidence
```
