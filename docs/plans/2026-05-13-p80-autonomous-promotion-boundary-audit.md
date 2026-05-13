# P80 Autonomous Promotion Boundary Audit

Spec: `specs/p80-autonomous-promotion-boundary-audit.spec.md`

## Checklist

- [x] Add P80 task contract and plan.
- [x] Update `docs/MIND-MODEL-DESIGN.md` with the autonomous promotion boundary.
- [x] Update `src/core/protocol.rs` so agents do not infer autonomous lifecycle authority.
- [x] Update AGENTS/CLAUDE spec and plan inventory.
- [x] Verify spec parse/lint, grep-based acceptance checks, fmt/check where relevant, and diff boundary.

## Verification

```bash
agent-spec parse specs/p80-autonomous-promotion-boundary-audit.spec.md
agent-spec lint specs/p80-autonomous-promotion-boundary-audit.spec.md --min-score 0.7
rg -n "P80 autonomous promotion boundary audit|human-gated lifecycle authority|autonomous promotion is out of scope" docs/MIND-MODEL-DESIGN.md
rg -n "must not autonomously promote|human/operator-triggered lifecycle mutation|evaluator advice remains advisory" src/core/protocol.rs
rg -n "p80-autonomous-promotion-boundary-audit|P80 autonomous promotion boundary audit" AGENTS.md CLAUDE.md
git diff --name-only main...HEAD
cargo fmt -- --check
cargo check
git diff --check
```
