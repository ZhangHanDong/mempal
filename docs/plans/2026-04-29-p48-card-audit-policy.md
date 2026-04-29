# P48 Card Audit Policy Plan

**Goal:** Resolve the Phase-2 card lifecycle JSONL audit question by keeping
`knowledge_events` authoritative and avoiding default transactional dual-write
to `audit.jsonl`.

## Checklist

- [x] Add P48 task contract.
- [x] Update MIND-MODEL with card audit policy.
- [x] Update AGENTS/CLAUDE inventories.
- [x] Run spec lint and docs-only verification.

## Verification Commands

```bash
agent-spec parse specs/p48-card-audit-policy.spec.md
agent-spec lint specs/p48-card-audit-policy.spec.md --min-score 0.7
rg -n "P48 keeps knowledge_events as the authoritative Phase-2 card audit trail|no default JSONL dual-write" docs/MIND-MODEL-DESIGN.md
rg -n "JSONL export.*derived from knowledge_events|not become a second source of truth" docs/MIND-MODEL-DESIGN.md
rg -n "p48-card-audit-policy|P48 card audit policy" AGENTS.md CLAUDE.md
rg -n "Do not write JSONL audit logs for Phase-2 card lifecycle" specs/p39-knowledge-card-lifecycle-cli.spec.md
git diff --name-only main...HEAD
git diff --check
```
