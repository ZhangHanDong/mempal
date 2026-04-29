# P49 Research Ingestion Policy Plan

**Goal:** Resolve the research-rs integration future-work item by defining how
external research output enters mempal without allowing research to directly
define `dao`.

## Checklist

- [x] Add P49 task contract.
- [x] Update MIND-MODEL with the research ingestion path and forbidden bypasses.
- [x] Update AGENTS/CLAUDE inventories.
- [x] Run spec lint and docs-only verification.

## Verification Commands

```bash
agent-spec parse specs/p49-research-ingestion-policy.spec.md
agent-spec lint specs/p49-research-ingestion-policy.spec.md --min-score 0.7
rg -n "P49 defines the research-rs ingestion path|memory_kind=evidence.*provenance=research|candidate knowledge only through distill" docs/MIND-MODEL-DESIGN.md
rg -n "research must not directly create dao_tian|must not directly create canonical or promoted knowledge|lifecycle gates" docs/MIND-MODEL-DESIGN.md
rg -n "p49-research-ingestion-policy|P49 research ingestion policy" AGENTS.md CLAUDE.md
rg -n "Provenance::Research|distill only allows candidate dao_ren or qi|evidence drawer accepts explicit runtime or research provenance" src tests
git diff --name-only main...HEAD
git diff --check
```
