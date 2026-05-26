# Maintenance Runbook

This runbook describes the governed maintenance loop for mempal's mind-model
and self-evolution substrate. It is a checklist, not an autonomous daemon.

## 1. Gather External Evidence

Use research tooling to produce a reproducible report, then validate the report
before ingesting anything:

```bash
mempal phase3 research-validate-plan report.json --format json
mempal phase3 research-ingest-plan report.json --format json
mempal phase3 research-ingest-plan report.json --execute --format json
```

Research output enters mempal as evidence or evidence-backed candidate
insights. It does not directly define `dao`.

## 2. Distill Candidate Knowledge

Use existing evidence refs to create candidate knowledge:

```bash
mempal knowledge distill \
  --tier dao_ren \
  --statement "A stable domain rule..." \
  --supporting-ref drawer_...
```

The result remains candidate until deterministic gates and human review allow
promotion.

## 3. Govern Cards And Lifecycle

Use knowledge-card gates before lifecycle changes:

```bash
mempal knowledge-card gate card_...
mempal knowledge-card promote card_... --target-status active --evidence-ref drawer_...
mempal knowledge-card demote card_... --status retired --reason "counterexample" --evidence-ref drawer_...
```

Cards are governed artifacts, not automatic beliefs.

## 4. Assemble Context And Record Adoption

Use context packs during real work and record observed outcomes:

```bash
mempal context "current task" --include-cards
mempal phase3 adoption capture \
  --surface card-context \
  --outcome accepted \
  --card-id card_... \
  --query "current task" \
  --execute
```

Runtime adoption evidence is the feedback layer that proves whether context
actually helped.

## 5. Review Readiness And Rollback

```bash
mempal phase3 adoption review --format json
mempal phase3 readiness card-context-default --format json
mempal phase3 default-proposal card-context-default \
  --rollback-criterion "disable if rejection rate exceeds threshold"
mempal phase3 rollback-control card-context --format json
```

Default changes require evidence and rollback criteria.

## 6. Maintain Cowork State

For multi-agent work, summarize operational state before ending a session:

```bash
mempal cowork-doctor --cwd "$PWD"
mempal cowork-handoff --cwd "$PWD" --format plain
mempal cowork-capture --cwd "$PWD" --summary-source handoff --execute
```

`cowork-capture` is explicit evidence capture. Runtime communication remains
ephemeral unless this bridge is invoked.

## 7. Auto-Dream Boundary

Auto-dream or manual dream can use this checklist, but mempal does not install
background maintenance workers. The governed loop is:

1. validate evidence
2. ingest evidence
3. distill candidates
4. gate lifecycle changes
5. use context
6. record runtime adoption
7. review and rollback when needed

No step grants autonomous promotion authority.
