# MIND MODEL DESIGN

**Date**: 2026-04-21
**Status**: Draft - discussion capture for review
**Scope**: Capture the mind-model decisions discussed in this conversation and map them to a practical system design.

## One-Sentence Thesis

The system should treat memory as a governed knowledge evolution layer where raw evidence is accumulated first, abstract knowledge is distilled second, and high-level `dao` is woken up before `shu` and `qi` at runtime.

## Goal

This design defines how to combine:

- memory
- skills
- external research tools
- runtime evaluation and promotion gates

into a coherent agent cognition system instead of treating them as unrelated parts.

The key idea is:

- memory is not just storage
- skills are not just static instructions
- research tools are not just retrieval utilities

Together they should form a disciplined loop:

1. gather evidence
2. distill candidate knowledge
3. promote only with sufficient evidence
4. demote when contradicted or obsolete

## Core Vocabulary

### Dao / Shu / Qi

This design adopts the following knowledge hierarchy:

- `dao`: high-level knowledge and governing principles
- `shu`: reusable methods, workflows, and procedural know-how
- `qi`: concrete tools, commands, interfaces, and tool-specific usage knowledge

`dao` itself has two levels:

- `dao_tian`: universal law; the most stable, cross-domain, objective knowledge
- `dao_ren`: domain law; stable patterns within a given field

Relationship:

- `dao_tian` shapes how the agent understands reality
- `dao_ren` shapes how the agent understands a specific field
- `shu` shapes how the agent acts
- `qi` shapes what the agent uses to act

### Memory Domains

These are independent from `dao / shu / qi`.

Memory domains answer: "who is this memory for?"

- `project`
- `agent`
- `skill`
- `global`

### Provenance

Every memory item should also record where it came from:

- `runtime`
- `research`
- `human`

## Crucial Orthogonality

`dao / shu / qi` is **not** the same axis as project memory, agent memory, or skill memory.

These are orthogonal coordinate systems:

1. `memory domain`
2. `knowledge tier`
3. `field`
4. `provenance`
5. `anchor`

Example:

- a debugging checklist may be `domain=skill`, `tier=shu`, `field=software-engineering`
- a model-specific CLI behavior note may be `domain=agent`, `tier=qi`, `field=tooling`
- a high-level principle like "evidence precedes assertion" may be `domain=global`, `tier=dao_tian`, `field=epistemics`

This orthogonality is required to avoid confusing:

- local project lessons with universal law
- temporary tool behavior with stable domain knowledge
- workflow tips with governing principles

## Anchor Model

Project identity should not be overloaded onto `wing`.

`wing` remains a semantic partitioning axis. It answers:

- what semantic area does this memory belong to?

It should not also answer:

- which checkout does this memory belong to?
- which branch experiment does this memory belong to?

That requires a separate anchor axis.

### Anchor Kinds

The recommended anchor model is:

- `global`
- `repo`
- `worktree`

Meaning:

- `global`: not tied to a repository checkout; used for cross-project memory, especially high-level `dao`
- `repo`: shared memory for the logical repository across branches and worktrees
- `worktree`: branch-local or experiment-local memory bound to one checkout path

### Why Worktree Must Exist

A repo-only project anchor is insufficient.

If memory is anchored only to the repo root:

- branch experiments contaminate each other
- temporary workflows and conclusions leak across unrelated checkouts
- failed experiments in one worktree pollute stable reasoning in another

Using the worktree path as a memory anchor preserves checkout-local memory.

### Why Worktree Alone Is Not Enough

A worktree-only anchor is also insufficient.

If memory is anchored only to worktree path:

- stable project knowledge fragments across checkouts
- verified project-wide `shu` and `dao_ren` become hard to share
- each new worktree starts too empty

Therefore the recommended model is dual-anchor, not worktree-only.

### Recommended Dual-Anchor Design

Every project-tied memory should be able to attach to:

- a `repo_anchor`
- optionally a `worktree_anchor`

This yields a useful separation:

- stable shared project memory lives at `repo`
- branch-local experiments, transient failures, and temporary heuristics live at `worktree`
- universal knowledge lives at `global`

### Anchor Does Not Replace Domain

Anchor and domain answer different questions.

- `domain` asks: who is this memory for?
- `anchor` asks: which persistence scope does it belong to?

Examples:

- a global epistemic law may be `domain=global`, `anchor=global`
- a project-wide build rule may be `domain=project`, `anchor=repo`
- a branch-local debugging lesson may be `domain=project`, `anchor=worktree`

### Stage-1 Anchor Fields

For the bootstrap drawer model, the minimum anchor fields should be:

- `anchor_kind`: `global | repo | worktree`
- `anchor_id`: normalized identifier for that anchor
- optional `parent_anchor_id`

The parent relationship is primarily for:

- `worktree -> repo`

This allows branch-local memory to inherit a stable project parent.

### Anchor Generation Rules

At stage 1, the anchor rules should be deterministic and filesystem-derived.

Recommended generation:

- `global`
  - fixed symbolic id, not derived from cwd
- `repo`
  - normalized repository identity
- `worktree`
  - normalized checkout identity

The critical rule is:

- do not derive anchor identity from `wing`

### Repo Anchor

The `repo_anchor` should identify the logical repository shared by all worktrees.

Recommended source:

- repository top-level path or a canonical repo identity derived from git metadata

The exact encoding can evolve, but the semantics should remain:

- all worktrees of the same repo map to the same `repo_anchor`

### Worktree Anchor

The `worktree_anchor` should identify a specific checkout path.

Recommended source:

- canonical worktree path

Semantics:

- different worktree paths produce different `worktree_anchor`s
- the main checkout and each extra git worktree are treated as separate worktree anchors

### Default Assignment Policy

At stage 1, memories should default to the following anchors:

- `dao_tian`
  - `global`
- stable `dao_ren`
  - usually `global` or `repo`, depending on whether it is cross-project or repo-specific
- project-shared `shu`
  - `repo`
- branch-local `qi` and experimental `shu`
  - `worktree`
- runtime observations from the current checkout
  - `worktree`
- external research evidence intended to support the current repo
  - `repo` by default, `worktree` when clearly branch-local

### Runtime Wake-Up with Anchors

When anchors are present, runtime assembly should prefer:

1. current `worktree`
2. current `repo`
3. `global`

This gives the agent:

- local experimental context first
- stable project memory second
- universal law last, but still available

This ordering complements the knowledge-tier wake-up order rather than replacing it.

In other words, anchor filtering and `dao / shu / qi` ordering are separate passes.

### Anchor Promotion Is Separate from Tier Promotion

The system needs two distinct upward movements:

1. knowledge-tier promotion
2. anchor-scope publication

These are not the same operation.

Examples:

- `qi @ worktree -> shu @ worktree` is a tier promotion
- `shu @ worktree -> shu @ repo` is an anchor publication

Recommended publication chain:

- `worktree -> repo -> global`

Meaning:

- new or experimental memory should usually start at `worktree`
- only verified, shareable project memory should move to `repo`
- only cross-project law should live at `global`

This yields the governing principle:

- write local first
- publish outward only after evidence justifies it

At stage 1, this can remain a disciplined workflow and data-model invariant even
if a full `publish_anchor` API does not exist yet.

## What Counts as Real Learning

The system should not consider "more stored text" to be the same as learning.

True learning happens only when:

1. observations accumulate in evidence memory
2. patterns are distilled into `qi` or `shu`
3. repeated and bounded patterns are promoted into `dao_ren`
4. only extremely stable, cross-domain knowledge is promoted into `dao_tian`

Therefore:

- `qi` can be accumulated quickly
- `shu` should be distilled with care
- `dao_ren` should be promoted rarely
- `dao_tian` should be promoted extremely rarely

## Layered Architecture

The system should be separated into four logical layers:

1. external tools
2. evidence memory
3. knowledge memory
4. runtime execution

### External Tools

Examples:

- `research-rs`
- CLI tools
- MCP tools
- test runners
- build tools

These belong to `qi`. They are capabilities, not high-level knowledge.

### Evidence Memory

This layer stores raw, source-backed observations.

Examples:

- research results
- runtime observations
- human explicit teachings
- concrete failures
- counterexamples
- contradictions

Evidence memory is allowed to contain inconsistent or conflicting facts.

That is expected. It reflects the world as observed.

### Knowledge Memory

This layer stores distilled, governed knowledge:

- `qi`
- `shu`
- `dao_ren`
- `dao_tian`

Knowledge memory should never be a raw dump of evidence. It is a controlled compilation layer built on top of evidence.

### Runtime Execution

This is where agents:

- wake up relevant knowledge
- choose the right skill
- bind to available tools
- act under constraints

## Evidence Memory vs Knowledge Memory

This split is mandatory.

If raw evidence and abstract knowledge are stored as the same thing forever, the system will quickly lose the distinction between:

- fact and conclusion
- observation and law
- candidate and canon

### Evidence Memory Principles

- raw-first
- source-backed
- append-friendly
- contradiction-tolerant
- high volume

### Knowledge Memory Principles

- distilled
- bounded
- stateful
- auditable
- lower volume

In short:

- evidence memory stores "what we saw"
- knowledge memory stores "what we therefore believe"

## Relationship Between Memory, Skills, and Research

### Memory

Memory is the governed persistence and wake-up system.

It should contain both:

- evidence memory
- knowledge memory

`dao` belongs here, not in external research tools.

### Skills

Skills primarily encode `shu`.

But a good skill should also expose the `dao` that justifies the workflow and the `qi` needed to execute it.

So a mature skill should be read as:

1. governing principle
2. workflow
3. tool binding

### Research Tools

External research tools do not define `dao`.

Their role is:

- fetch evidence
- structure evidence
- help verify or falsify existing knowledge

So `research-rs` is `qi`, and its output primarily feeds evidence memory.

## research-rs Boundary

`research-rs` is an external tool. It should not be given responsibility for maintaining `dao`.

Its appropriate role is close to the `LLM Wiki` pattern:

- raw sources
- wiki
- schema
- index
- log
- lint

But the outputs of `research-rs` should be treated as:

- evidence
- structured summaries
- candidate insights
- contradiction signals

They are not automatically `dao`.

Therefore:

- `research-rs` organizes the external world
- memory governs what is promoted from those results

## Runtime Wake-Up Order

The runtime order should be explicit, not left to ad hoc semantic retrieval.

Recommended order:

1. `dao_tian`
2. `dao_ren`
3. `shu`
4. `qi`
5. `evidence`

Rationale:

- `dao_tian` calibrates the agent's worldview
- `dao_ren` calibrates the current field
- `shu` proposes methods
- `qi` binds execution to available tools
- `evidence` is used for grounding, exception handling, and proof

This order should not imply that the system always injects all layers.

Rather:

- use `dao_tian` sparingly and only when truly needed
- use `dao_ren` based on the active field
- use `shu` as the main skill trigger and execution layer
- use `qi` only when binding to concrete capabilities
- use evidence when verification or exception-handling is necessary

## Promotion Hierarchy

Knowledge should evolve through controlled promotion, not direct assertion.

Recommended conceptual path:

- `observation -> qi/shu`
- `shu -> dao_ren`
- `dao_ren -> dao_tian`

With the following meaning:

- `qi`: tool-bound knowledge
- `shu`: repeatable method
- `dao_ren`: domain law
- `dao_tian`: universal law

Higher promotion requires:

- fewer entries
- stronger evidence
- broader validity
- clearer boundaries
- stronger review

## Promotion Gate Philosophy

The system should never let "the agent found something interesting" equal "the system learned a law".

Instead:

- research and runtime can produce evidence quickly
- candidate knowledge can be distilled frequently
- promotion must be gated
- high-level law must be rare

This design strongly favors:

- fast evidence growth
- slow law growth

## Knowledge Lifecycle

The knowledge layer should support at least these states:

- `candidate`
- `promoted`
- `canonical`
- `demoted`
- `retired`

Meaning:

- `candidate`: not yet trusted for default runtime wake-up
- `promoted`: trusted enough for ordinary use
- `canonical`: highly stable and preferred
- `demoted`: weakened by stronger evidence or invalidation
- `retired`: no longer active, retained only for audit and history

Important rule:

High-level knowledge must be reversible. Promotion without demotion leads to knowledge pollution.

## Four Core Operations

The smallest viable lifecycle should be modeled through four operations:

1. `record`
2. `distill`
3. `promote`
4. `demote`

### record

Store raw evidence.

Examples:

- research result
- runtime failure
- human teaching
- observed contradiction

### distill

Create a candidate knowledge item from evidence.

Examples:

- tool usage note
- workflow heuristic
- domain pattern candidate

### promote

Move candidate knowledge into active runtime use once its gate is satisfied.

### demote

Reduce or retire knowledge when it is contradicted, superseded, or becomes outdated.

## Minimal Data Shape

### Evidence Memory

Evidence entries should be raw-first and source-backed.

Suggested fields:

- `id`
- `content`
- `domain`
- `field`
- `provenance`
- `source_ref`
- `timestamp`
- `tags`

### Knowledge Memory

Knowledge entries should be explicit and auditable.

Suggested fields:

- `id`
- `statement`
- `tier`
- `domain`
- `field`
- `status`
- `stability`
- `evidence_refs`
- `scope_constraints`
- `counterexamples`
- `promotion_history`

## Stage-1 Bootstrap Drawer Schema

Phase 1 should reuse the existing drawer system, but not by pretending all
drawers mean the same thing.

The bootstrap model should explicitly separate:

- `evidence drawer`
- `knowledge drawer`

### Shared Stage-1 Fields

Both drawer kinds should share the current base drawer fields and add:

- `memory_kind`: `evidence | knowledge`
- `domain`: `project | agent | skill | global`
- `field`
- `anchor_kind`: `global | repo | worktree`
- `anchor_id`
- optional `parent_anchor_id`

These fields should be explicit, not hidden inside JSON blobs, because they are
part of query-time filtering and runtime wake-up assembly.

### Evidence Drawer

The minimum stage-1 evidence drawer should add:

- `memory_kind='evidence'`
- `domain`
- `field`
- `provenance`: `runtime | research | human`
- `anchor_kind`
- `anchor_id`

Evidence drawers should *not* carry knowledge-governance fields such as:

- `tier`
- `status`
- `statement`
- `trigger_hints`
- role-separated knowledge refs

Evidence drawers record what was seen, taught, verified, or contradicted. They
can use tags to indicate whether they are supporting evidence, a boundary case,
or a counterexample, but they are not themselves promoted knowledge.

### Knowledge Drawer

The minimum stage-1 knowledge drawer should add:

- `memory_kind='knowledge'`
- `domain`
- `field`
- `statement`
- `tier`: `qi | shu | dao_ren | dao_tian`
- `status`: `candidate | promoted | canonical | demoted | retired`
- `supporting_refs`
- `counterexample_refs`
- `teaching_refs`
- `verification_refs`
- `scope_constraints`
- `trigger_hints`
- `anchor_kind`
- `anchor_id`
- optional `parent_anchor_id`

For knowledge drawers:

- `content` is the longer explanatory body
- `statement` is the short wake-up form

### Why Evidence Refs Must Be Role-Separated

Stage 1 should not collapse all evidence into one undifferentiated
`evidence_refs` list.

The minimum useful split is:

- `supporting_refs`
- `counterexample_refs`
- `teaching_refs`
- `verification_refs`

This matters because the runtime and future evaluator must be able to
distinguish:

- what supports a knowledge claim
- what limits it
- what was explicitly taught by a human
- what was actively re-verified rather than merely observed

### Minimal Trigger Hints

Stage 1 should allow a very small `trigger_hints` object for knowledge drawers,
but it must remain a bias layer, not a second skill registry.

The allowed structure should be limited to:

- `intent_tags`
- `workflow_bias`
- `tool_needs`

It should not directly name hard skill ids or become the authoritative trigger
mechanism.

### Statement vs Content

`statement` and `content` should have different jobs.

Recommended rule:

- `statement` is the short, directly wakeable knowledge proposition
- `content` is the explanatory body with rationale, boundaries, and clarifying detail

Therefore:

- `statement` should not contain extended justification, examples, or long scope notes
- `content` should not merely restate `statement`

This supports a clean runtime pattern:

1. wake by `statement`
2. drill into `content` only when explanation, review, or adjudication is needed

### Natural Status Distribution by Tier

The stage-1 model should expect different status distributions for each tier:

- `dao_tian`: usually `canonical` or `demoted`
- `dao_ren`: usually `candidate` or `promoted`
- `shu`: usually `promoted`
- `qi`: usually `candidate` or `promoted`

This is not merely stylistic. It reflects the intended rarity and stability of
each layer.

## Minimal Interface Surface

If exposed through memory APIs, the minimal operations should roughly map to:

- `record(content, domain, field, provenance, source_ref, tags?)`
- `distill(statement, tier_candidate, domain, field, evidence_refs, scope_constraints, rationale, counterexamples?)`
- `promote(knowledge_id, target_status, promotion_reason, validation_refs, reviewer)`
- `demote(knowledge_id, reason_type, reason, evidence_refs, target_status)`

Design principle:

The caller should not self-score high-level confidence directly. Confidence and stability should be system-derived or gate-derived whenever possible.

## Two-Phase Implementation Strategy

The recommended implementation path is two-stage.

### Phase 1: Drawer Bootstrap

Use two drawer types:

- `evidence drawer`
- `knowledge drawer`

Purpose:

- validate the value of the model quickly
- avoid a large rewrite
- reuse the current storage, ingest, and search paths

Limits:

- knowledge drawers will eventually become overloaded with lifecycle and governance metadata
- this is a bootstrap architecture, not the final form

Implemented Phase-1 runtime surface:

- `mempal context <query>` assembles a runtime context pack from typed drawers
- `mempal_context` exposes the same pack to MCP-connected agents
- knowledge sections are ordered as `dao_tian -> dao_ren -> shu -> qi`
- Stage-1 field taxonomy is guidance-only and read-only: `mempal field-taxonomy`
  and `mempal_field_taxonomy` expose recommended fields such as `general`,
  `epistemics`, `software-engineering`, `debugging`, `tooling`, `research`,
  `writing`, and `diary`, while custom field strings remain valid
- `dao_tian` is sparse by default in runtime context: `mempal context` and
  `mempal_context` inject at most 1 `dao_tian` item unless the caller explicitly
  sets `--dao-tian-limit` / `dao_tian_limit`; `0` disables `dao_tian`
- `wake-up` remains an L0/L1 memory refresh surface and does not assemble typed
  `dao_tian -> dao_ren -> shu -> qi` sections; typed operating guidance belongs
  to `mempal context` / `mempal_context`
- evidence remains opt-in via `--include-evidence`
- same-tier items prefer `worktree`, then current `repo`, then `repo://legacy`, then `global`
- `global` anchor candidates use `domain=global`, preserving the invariant that global anchors do not hold project-local domain memory
- `trigger_hints` are exposed as metadata only; they do not directly execute skills
- MCP protocol guidance consumes context in order: read `dao_tian` and `dao_ren` for judgment, use `shu` to bias workflow / skill choice, and use `qi` to bias concrete tool choice
- memory hints never override system, user, repo, or client-native skill rules
- bootstrap distill CLI creates candidate `dao_ren` / `qi` knowledge drawers from existing evidence refs without auto-promotion or LLM summarization
- `mempal_knowledge_distill` exposes the same deterministic distill operation to MCP-connected agents, letting runtime agents create candidate knowledge from evidence refs without shelling out
- bootstrap lifecycle CLI supports manual `promote` / `demote` on existing knowledge drawers by updating status plus verification / counterexample refs and writing audit entries
- lifecycle verification / counterexample refs are hardened to require existing evidence drawers, preserving the rule that promotion and demotion are justified by evidence rather than arbitrary ids or other knowledge claims
- promotion gate CLI provides a read-only advisory report before promotion, using deterministic evidence-count policy without mutating status, refs, vectors, schema, or audit history
- `mempal_knowledge_gate` exposes the same read-only promotion gate to MCP-connected agents, so runtime agents can check readiness without shelling out or mutating lifecycle state
- Stage-1 promotion policy is inspectable without a concrete drawer through `mempal knowledge policy` and `mempal_knowledge_policy`
- current Stage-1 thresholds are:
  - `dao_tian -> canonical`: 3 supporting refs, 2 verification refs, 1 teaching ref, human reviewer required, counterexamples block
  - `dao_ren -> promoted`: 2 supporting refs, 1 verification ref, counterexamples block
  - `shu -> promoted`: 1 supporting ref, 1 verification ref, counterexamples block
  - `qi -> promoted`: 1 supporting ref, 1 verification ref, counterexamples block
- `dao_tian -> canonical` always requires a human reviewer in Stage 1; evaluator-only canonization is intentionally out of scope
- `mempal_knowledge_promote` and `mempal_knowledge_demote` expose lifecycle mutation to MCP-connected agents; promotion is gate-enforced after appending supplied verification refs, while demotion requires counterexample evidence
- `mempal knowledge publish-anchor` implements explicit outward anchor publication for active knowledge (`worktree -> repo -> global`) as a metadata-only operation separate from tier/status promotion
- `mempal_knowledge_publish_anchor` exposes the same outward anchor publication operation to MCP-connected agents without changing content, vectors, tier, or status
- lifecycle updates are metadata-only in Stage 1; they do not rewrite content, re-embed vectors, or create Phase-2 knowledge cards

### Phase 2: Knowledge Card Extraction

Once the model proves useful, separate knowledge memory from evidence memory structurally.

Recommended objects:

- `drawers` for evidence
- `knowledge_cards`
- `knowledge_evidence_links`
- `knowledge_events`

This yields a cleaner separation:

- evidence says what happened
- knowledge says what is believed
- events say how that belief evolved

## Decision on Bootstrap vs Final Architecture

Current recommendation:

- start with two drawer types
- explicitly mark this as bootstrap-only
- plan for extraction into separate knowledge objects later

This gives the system a low-cost learning path without pretending the temporary structure is ideal.

## What Belongs Where

### In research-rs

- ingest and normalize external sources
- maintain research wiki/index/log/lint
- emit structured evidence and candidate insights

### In memory layer

- store evidence memory
- store knowledge memory
- maintain `dao / shu / qi`
- manage promotion and demotion lifecycle

### In skills

- encode reusable workflows
- expose relevant `dao`
- bind to `qi`

### In evaluator/gate

- validate promotions
- handle demotions
- guard against self-pollution

## Non-Goals

This design does not assume:

- fully automatic promotion to `dao`
- external research directly creating universal law
- replacing raw evidence with compressed knowledge
- collapsing evidence, knowledge, and workflow into one storage object forever

## Open Questions

The following remain open and should be resolved in later design work:

1. When Phase 2 begins, should knowledge cards live in the same DB or a separate persistence layer?

## Current Recommendation

Proceed with the following assumptions unless future evidence rejects them:

- `dao` belongs to the memory layer
- `research-rs` is an external `qi` tool, not a `dao` container
- evidence memory and knowledge memory should be explicitly separated
- runtime typed context should assemble `dao` before `shu`, and `shu` before
  `qi`; wake-up remains a refresh surface, not the typed assembler
- the implementation should begin with drawer bootstrap and evolve into a dedicated knowledge model

## Closing Summary

The proposed system is not "RAG plus skills."

It is a governed cognition stack:

- external tools gather and organize evidence
- memory stores both evidence and distilled knowledge
- skills operationalize methods under governing principles
- evaluators control what is allowed to harden into lasting law

That is the intended meaning of this design:

- `dao` is memory-level high-order knowledge
- `shu` is operational method
- `qi` is executable capability
- evidence is the substrate from which all of them must be justified
