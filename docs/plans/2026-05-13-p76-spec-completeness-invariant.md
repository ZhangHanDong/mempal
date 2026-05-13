# P76 Spec Completeness Invariant

Goal: codify the project governance rule that every numbered P must leave both
a task spec and a matching plan before it can be considered complete.

Spec: `specs/p76-spec-completeness-invariant.spec.md`

## Steps

- [x] Add P76 task contract and plan.
- [x] Update AGENTS/CLAUDE Spec system rules with the hard invariant.
- [x] Add P76 to completed spec and plan inventories.
- [x] Add a MIND-MODEL governance note and renumber the next recommended P
      candidates after reserving P76.
- [x] Verify spec parse/lint and grep selectors.
- [x] Verify P76 is governance-only with changed-file boundaries.
- [ ] Commit, ingest decision memory, push, and open/merge PR.

## Verification

```bash
agent-spec parse specs/p76-spec-completeness-invariant.spec.md
agent-spec lint specs/p76-spec-completeness-invariant.spec.md --min-score 0.7
rg -n "Every future numbered P must include|docs/plans/.*pNN|documentation-only, audit-only, policy-only" specs/p76-spec-completeness-invariant.spec.md
rg -n "每一个 numbered P|specs/pNN-\\*\\.spec\\.md|docs/plans/.*pNN|文档-only|audit-only|policy-only" AGENTS.md CLAUDE.md
rg -n "p76-spec-completeness-invariant|P76 spec completeness invariant" AGENTS.md CLAUDE.md
rg -n "P76 spec completeness invariant|Every P must leave a spec|future P numbering" docs/MIND-MODEL-DESIGN.md
rg -n "不能算完成|spec-less P|missing spec|必须留下.*spec" AGENTS.md CLAUDE.md docs/MIND-MODEL-DESIGN.md
git diff --name-only main...HEAD
git diff --check
```
