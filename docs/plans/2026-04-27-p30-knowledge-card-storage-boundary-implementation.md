# P30 Knowledge Card Storage Boundary Implementation Plan

**Goal:** Resolve the last MIND-MODEL-DESIGN open question: Phase-2
`knowledge_cards` should live in the same SQLite `palace.db`, but in separate
tables from evidence drawers.

**Architecture:** This is a design-boundary task only. Update specs and docs so
future implementation has a fixed persistence decision, while explicitly keeping
Phase-2 schema, migrations, and runtime surfaces out of scope.

## Steps

- [x] Validate task contract with `agent-spec parse/lint`.
- [x] Update MIND-MODEL-DESIGN Phase-2 storage decision.
- [x] Remove the resolved Open Questions section.
- [x] Update usage docs and repository agent inventories.
- [x] Verify no Phase-2 schema/runtime implementation was introduced.

## Verification

```bash
agent-spec parse specs/p30-knowledge-card-storage-boundary.spec.md
agent-spec lint specs/p30-knowledge-card-storage-boundary.spec.md --min-score 0.7
rg -n "same SQLite .*palace.db|knowledge_cards.*same SQLite" docs/MIND-MODEL-DESIGN.md
! rg -n "^## Open Questions|should knowledge cards live" docs/MIND-MODEL-DESIGN.md
rg -n "Phase-2 .*same SQLite palace.db|not implemented" docs/usage.md
rg -n "p30-knowledge-card-storage-boundary|P30 knowledge card storage boundary" AGENTS.md CLAUDE.md
! rg -n "CREATE TABLE knowledge_cards|struct KnowledgeCard|mempal_knowledge_card" src tests
```
