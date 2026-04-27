# P43 Knowledge Card Retrieval Contract Plan

**Goal:** Define how Phase-2 knowledge cards should be consumed as runtime
retrieval results before adding any card-aware context/search behavior.

## Steps

- [x] Add P43 task contract.
- [x] Update MIND-MODEL design with the card retrieval contract.
- [x] Update AGENTS / CLAUDE inventories.
- [x] Run spec parse/lint and documentation checks.

## Verification

```bash
agent-spec parse specs/p43-knowledge-card-retrieval-contract.spec.md
agent-spec lint specs/p43-knowledge-card-retrieval-contract.spec.md --min-score 0.7
rg -n "card_id.*statement.*content|evidence_drawer_id.*role.*source_file" docs/MIND-MODEL-DESIGN.md
rg -n "promoted.*canonical|candidate.*demoted.*retired" docs/MIND-MODEL-DESIGN.md
rg -n "P43 does not change.*mempal context|P43 does not change.*mempal_search" docs/MIND-MODEL-DESIGN.md
rg -n "p43-knowledge-card-retrieval-contract" AGENTS.md CLAUDE.md
```
