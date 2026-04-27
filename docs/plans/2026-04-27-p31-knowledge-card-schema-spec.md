# P31 Knowledge Card Schema Spec Plan

**Goal:** Define the minimum Phase-2 schema contract for `knowledge_cards`,
`knowledge_evidence_links`, and `knowledge_events` before implementing schema v8.

**Architecture:** P31 is spec-only. It fixes table shape, constraints, indexes,
and future acceptance tests, while explicitly forbidding migrations or runtime
surfaces in this PR.

## Steps

- [x] Draft `specs/p31-knowledge-card-schema.spec.md`.
- [x] Update MIND-MODEL-DESIGN with the schema v8 draft.
- [x] Register P31 as the current draft spec in AGENTS and CLAUDE.
- [x] Validate the spec with `agent-spec parse/lint`.
- [x] Verify no schema/runtime implementation was introduced.

## Verification

```bash
agent-spec parse specs/p31-knowledge-card-schema.spec.md
agent-spec lint specs/p31-knowledge-card-schema.spec.md --min-score 0.7
rg -n "knowledge_cards|knowledge_evidence_links|knowledge_events|schema v8" docs/MIND-MODEL-DESIGN.md
rg -n "p31-knowledge-card-schema|schema v8|未实现" AGENTS.md CLAUDE.md
! rg -n "CREATE TABLE knowledge_cards|struct KnowledgeCard|mempal_knowledge_card" src tests
```
