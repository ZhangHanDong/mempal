# P46 Card Context Default Policy Plan

**Goal:** Resolve the next MIND-MODEL future-work item by keeping card-aware
context opt-in and defining the evidence required before any future default
enablement.

## Checklist

- [x] Add P46 task contract.
- [x] Update MIND-MODEL with the default-context policy and future evidence gate.
- [x] Update AGENTS/CLAUDE inventories.
- [x] Run spec lint and docs-only verification.

## Verification Commands

```bash
agent-spec parse specs/p46-card-context-default-policy.spec.md
agent-spec lint specs/p46-card-context-default-policy.spec.md --min-score 0.7
rg -n "P46 keeps card-aware context opt-in|default context remains drawer-only" docs/MIND-MODEL-DESIGN.md
rg -n "Evidence required before default enablement|rollback criteria|context bloat" docs/MIND-MODEL-DESIGN.md
rg -n "p46-card-context-default-policy|P46 card context default policy" AGENTS.md CLAUDE.md
git diff --name-only main...HEAD
git diff --check
```
