# P47 Card Embedding Policy Plan

**Goal:** Resolve the card embedding future-work item by keeping card-level
embeddings deferred and defining the evidence/schema requirements for any later
implementation.

## Checklist

- [x] Add P47 task contract.
- [x] Update MIND-MODEL with the card embedding deferral policy.
- [x] Update AGENTS/CLAUDE inventories.
- [x] Run spec lint and docs-only verification.

## Verification Commands

```bash
agent-spec parse specs/p47-card-embedding-policy.spec.md
agent-spec lint specs/p47-card-embedding-policy.spec.md --min-score 0.7
rg -n "P47 keeps card-level embeddings deferred|linked-evidence retrieval remains the only implemented card retrieval strategy" docs/MIND-MODEL-DESIGN.md
rg -n "Evidence required before card embeddings|statement-match misses|stale-vector handling|rollback behavior" docs/MIND-MODEL-DESIGN.md
rg -n "p47-card-embedding-policy|P47 card embedding policy" AGENTS.md CLAUDE.md
git diff --name-only main...HEAD
! rg -n "knowledge_card_vectors|card vector|card_vectors" src
git diff --check
```
