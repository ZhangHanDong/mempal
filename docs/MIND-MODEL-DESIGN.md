# MIND MODEL DESIGN

**Date**: 2026-04-21
**Status**: P42 baseline implemented - future work remains explicit
**Scope**: Capture the mind-model decisions discussed in this conversation and map them to a practical system design.

## Implementation Checkpoint

P42 baseline means the core mind-model architecture is implemented enough to
operate as a governed memory system:

- Stage 1 typed drawers separate raw evidence from governed knowledge.
- `dao_tian -> dao_ren -> shu -> qi` runtime context assembly exists through
  `mempal context` and `mempal_context`.
- Stage 1 knowledge supports distill, gate, promote, demote, and outward anchor
  publication through CLI and MCP surfaces.
- Phase-2 `knowledge_cards`, `knowledge_evidence_links`, and
  `knowledge_events` exist in the same SQLite `palace.db`.
- Stage-1 knowledge drawers can be backfilled into Phase-2 cards through an
  explicit dry-run-first apply command.
- Phase-2 cards now have governed gate, promote, and demote lifecycle surfaces
  in CLI and MCP.

P42 baseline is not a claim that every future runtime integration is complete.
It marks the point where the design is no longer only a discussion capture: the
main storage, governance, and lifecycle surfaces exist and are test-backed.

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

P49 defines the research-rs ingestion path:

- raw/source research output enters as `memory_kind=evidence` with `provenance=research`
- structured summaries from research remain evidence unless explicitly distilled
- candidate knowledge only through distill from existing evidence refs
- contradiction signals become evidence or counterexamples for later demotion or
  gate evaluation

research must not directly create dao_tian. Research must not directly create
canonical or promoted knowledge. It must not bypass lifecycle gates. The highest
trust level research can supply by itself is source-backed evidence; memory owns
distillation, promotion, demotion, and canonicalization.

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

Minimum schema v8 draft:

`knowledge_cards`:

- `id TEXT PRIMARY KEY`
- `statement TEXT NOT NULL`
- `content TEXT NOT NULL`
- `tier TEXT NOT NULL CHECK ('qi','shu','dao_ren','dao_tian')`
- `status TEXT NOT NULL CHECK ('candidate','promoted','canonical','demoted','retired')`
- `domain TEXT NOT NULL CHECK ('project','agent','skill','global')`
- `field TEXT NOT NULL DEFAULT 'general'`
- `anchor_kind TEXT NOT NULL CHECK ('global','repo','worktree')`
- `anchor_id TEXT NOT NULL`
- `parent_anchor_id TEXT`
- `scope_constraints TEXT`
- `trigger_hints TEXT`
- `created_at TEXT NOT NULL`
- `updated_at TEXT NOT NULL`

`knowledge_evidence_links`:

- `id TEXT PRIMARY KEY`
- `card_id TEXT NOT NULL`
- `evidence_drawer_id TEXT NOT NULL`
- `role TEXT NOT NULL CHECK ('supporting','verification','counterexample','teaching')`
- `note TEXT`
- `created_at TEXT NOT NULL`
- unique key: `(card_id, evidence_drawer_id, role)`

`knowledge_events`:

- `id TEXT PRIMARY KEY`
- `card_id TEXT NOT NULL`
- `event_type TEXT NOT NULL CHECK ('created','promoted','demoted','retired','linked','unlinked','updated','published_anchor')`
- `from_status TEXT`
- `to_status TEXT`
- `reason TEXT NOT NULL`
- `actor TEXT`
- `metadata TEXT`
- `created_at TEXT NOT NULL`

Minimum indexes:

- `knowledge_cards(tier, status)`
- `knowledge_cards(domain, field)`
- `knowledge_cards(anchor_kind, anchor_id)`
- `knowledge_evidence_links(card_id)`
- `knowledge_evidence_links(evidence_drawer_id)`
- `knowledge_events(card_id, created_at)`

This yields a cleaner separation:

- evidence says what happened
- knowledge says what is believed
- events say how that belief evolved

Storage decision:

- Phase-2 `knowledge_cards` should live in the same SQLite `palace.db`
- they should be separate tables from `drawers`, not overloaded drawer rows
- `drawers` remain the raw evidence and citation root
- `knowledge_evidence_links` should reference evidence drawers by `drawer_id`
- `knowledge_events` should be transactional with knowledge-card lifecycle
  changes and evidence-link mutations
- a separate persistence layer is out of scope unless future measured needs
  prove the single-file SQLite boundary insufficient

Rationale:

- mempal's product invariant is a local single-binary, single-file memory palace
- knowledge promotion/demotion must stay transactionally tied to evidence refs
- citations remain simpler and safer when evidence drawer ids are the durable root
- using a second database or service would add operational complexity before the
  Phase-2 model has proven it needs independent scaling

Implemented Phase-2 surface at P42 baseline:

- `knowledge_cards`, `knowledge_evidence_links`, and `knowledge_events` are
  schema v8 tables in `palace.db`
- Rust core APIs can create/read/update/list cards, link evidence, and append
  events
- `mempal knowledge-card` exposes create/get/list/link/event/events
- `mempal_knowledge_cards` exposes list/get/events to MCP-connected agents
- `mempal knowledge-card backfill-plan` reports Stage-1 knowledge drawers that
  are ready to become cards without writing
- `mempal knowledge-card backfill-apply` defaults to dry-run and only writes
  cards, links, and created events with `--execute`
- `mempal knowledge-card gate` evaluates card readiness from role-separated
  evidence links
- `mempal knowledge-card promote` and `mempal knowledge-card demote` mutate
  card status transactionally with role-specific evidence links and append-only
  events
- `mempal_knowledge_cards` also exposes `gate`, `promote`, and `demote` actions
  over the same core lifecycle logic

Phase-2 cards are governed objects, but they are not yet the default
context/search source. At P42, `mempal context`, `mempal_context`, and
`mempal_search` remains drawer/citation based. Cards now have an explicit
linked-evidence retrieval path, but they are still not returned by default
search.

### Phase-2 Card Retrieval Contract

P43 defines the contract for future card-aware runtime consumption without
implementing retrieval behavior yet.

A card retrieval item is a governed knowledge result, not a raw drawer result.
The minimum returned card fields are: `card_id`, `statement`, `content`, `tier`,
`status`, `domain`, `field`, `anchor_kind`, and `anchor_id`.

Each card retrieval item must expose role-separated evidence citations derived
from `knowledge_evidence_links`. The minimum evidence citation fields are:
`evidence_drawer_id`, `role`, and `source_file`.

Default runtime eligibility is status-gated:

- `promoted` and `canonical` cards are runtime-eligible by default
- `candidate`, `demoted`, and `retired` cards are excluded by default

This preserves the governance boundary:

- card records carry distilled belief
- linked evidence drawers remain the citation root
- inactive card states remain inspectable but are not injected into ordinary
  runtime context

P43 does not change `mempal context` or `mempal_context` behavior.
P43 does not change `mempal_search` behavior.
Card embeddings, ranking strategy, and card-aware context/search surfaces are
deferred to later specs.

P44 adds the first explicit card-aware context surface:

- `mempal context --include-cards`
- `mempal_context` with `include_cards=true`

This remains opt-in. Default context assembly is still drawer-only. When enabled,
active Phase-2 cards are appended inside the existing
`dao_tian -> dao_ren -> shu -> qi` sections and expose `card_id` plus
role-separated `evidence_citations`. Each citation keeps the evidence drawer as
the citation root through `evidence_drawer_id`, `role`, and `source_file`.

P44 does not change `mempal_search`, does not add card embeddings, and does not
make cards the default runtime source.

P45 chooses the first card retrieval strategy:

- `mempal knowledge-card retrieve <query>`
- `mempal_knowledge_cards` with `action="retrieve"`

The strategy is linked-evidence-first. It searches evidence drawers through the
existing BM25+vector drawer search path, follows `knowledge_evidence_links`, and
returns active cards linked to matched evidence. Returned card items include the
card record, a score derived from matched evidence, and role-separated evidence
citations with `evidence_drawer_id`, `role`, `source_file`, and score.

P45 intentionally does not add card embeddings, does not add card vector
storage, and does not make `mempal_search` return cards.

P46 keeps card-aware context opt-in. The default context remains drawer-only for
both `mempal context` and `mempal_context`; operators must still pass
`--include-cards` or `include_cards=true` to inject Phase-2 cards into the
typed context pack.

This is a deliberate default policy, not an unfinished implementation gap.
Cards are now retrievable and context-injectable, but default runtime context is
a high-trust path. It should not silently switch from drawer-backed active
knowledge to mixed drawer/card guidance until real runtime evidence shows the
change improves agent behavior without weakening citations.

Evidence required before default enablement:

- repeated runtime traces where card-aware context causes better skill/tool
  selection than drawer-only context
- no observed citation loss: every default card item must preserve linked
  evidence citations as the citation root
- no material context bloat: card items must not crowd out higher-priority
  `dao_tian`, `dao_ren`, `shu`, or `qi` guidance
- no lifecycle confusion: inactive cards must remain excluded and demoted cards
  must not re-enter default context through linked evidence
- explicit rollback criteria: a future default-on spec must define how to return
  to drawer-only defaults if card injection degrades runtime behavior

P47 keeps card-level embeddings deferred. P45 linked-evidence retrieval remains
the only implemented card retrieval strategy: cards are found through matched
evidence drawers, not through a separate card vector index.

This keeps the citation model simple. Card statements are distilled beliefs;
evidence drawers remain the source-backed material. A card embedding index would
make card statements directly retrievable, which may improve recall, but it also
adds a new stale-vector surface and can make unsupported belief text feel like a
primary source unless every result still carries linked evidence citations.

Evidence required before card embeddings:

- statement-match misses: repeated retrieval traces where linked-evidence search
  misses useful active cards because evidence wording does not match the query
  but the card statement does
- citation preservation: card-embedding results must still return linked
  evidence citations as the citation root
- measurable recall improvement over P45 linked-evidence retrieval without
  unacceptable precision loss
- schema and maintenance plan for card vector storage, reindexing, and
  stale-vector handling
- rollback behavior that can disable card-vector retrieval and fall back to P45
  linked-evidence retrieval without data loss

P48 keeps `knowledge_events` as the authoritative Phase-2 card audit trail, with
no default JSONL dual-write for card lifecycle mutations. This keeps card
promote/demote/backfill behavior transactionally bound to the same SQLite
database that owns `knowledge_cards` and `knowledge_evidence_links`.

Stage-1 drawer lifecycle continues to use `audit.jsonl` where already defined.
Phase-2 card lifecycle does not mirror those entries into `audit.jsonl` by
default because that would create two audit surfaces with different durability
and transaction semantics. The append-only `knowledge_events` table is the
source of truth for card lifecycle history.

If an external integration needs JSONL card history, it should be added as an
explicit export surface. JSONL export must be derived from `knowledge_events`,
must be reproducible, and must not become a second source of truth.

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

## Current Recommendation

Proceed with the following assumptions unless future evidence rejects them:

- `dao` belongs to the memory layer
- `research-rs` is an external `qi` tool, not a `dao` container
- evidence memory and knowledge memory should be explicitly separated
- runtime typed context should assemble `dao` before `shu`, and `shu` before
  `qi`; wake-up remains a refresh surface, not the typed assembler
- the implementation should begin with drawer bootstrap and evolve into a
  dedicated knowledge model inside the same SQLite `palace.db`

## Future Work After P42

P42 originally left one explicit follow-up:

- add evaluator-assisted promotion only behind deterministic gates and human
  review rules for high-level knowledge

P50 closes that item as policy. P50 defines evaluator-assisted promotion as advisory-only.
Evaluators are not lifecycle actors.

Evaluators may:

- recommend promotion or demotion candidates
- propose supporting, verification, teaching, and counterexample evidence refs
- produce contradiction candidates and risk notes
- explain why a knowledge item appears ready or unsafe

Evaluators must not directly mutate status or otherwise act as lifecycle writers:

- append lifecycle refs as authoritative gate input
- bypass deterministic promotion or demotion gates
- satisfy reviewer requirements by evaluator-only review
- create automatic promotion or demotion paths

The deterministic gates remain authoritative. Promotion and demotion still go
through the existing gate-enforced CLI/MCP lifecycle surfaces. `dao_tian`
canonicalization still requires a human reviewer; evaluator-only canonization is forbidden.
If a future implementation adds evaluator APIs, that work must be a separate
spec and preserve deterministic replay, evidence citation, and audit semantics.

No open Future Work remains in the P42 list.

## Completion Status After P50

P51 closure audit: the MIND-MODEL baseline is complete.

No open implementation tasks remain in the P12-P50 baseline. The completed
baseline includes:

- typed evidence and knowledge drawers
- `dao_tian`, `dao_ren`, `shu`, and `qi` governance boundaries
- worktree/repo/global anchor behavior
- wake-up/context separation
- context-guided skill/tool selection
- distill, gate, promote, demote, and anchor publication lifecycle surfaces
- Phase-2 knowledge card storage, lifecycle, retrieval, and opt-in context
- research ingestion and evaluator promotion policies

Completion does not mean every optional future enhancement is implemented. It
means the current design baseline has no known open implementation task. Future
evaluator APIs, card-level embeddings, default card context, research adapters,
or other expansions must start as new-stage specs with their own evidence,
rollback criteria, and acceptance checks.

## Phase 3 Intake Roadmap

P52 Phase-3 intake roadmap defines how work starts after baseline closure.
Phase 3 is new-stage work, not unfinished P12-P50 baseline work.

Candidate tracks:

- evaluator APIs
- card retrieval maturity
- research adapter ingestion
- runtime adoption evidence

Intake rules:

- each candidate must state evidence, rollback criteria, and acceptance checks before implementation begins
- default-enabling card context or card embeddings requires measured retrieval benefit
- Evaluator APIs must preserve the P50 advisory-only lifecycle boundary
- Research adapters must preserve the P49 evidence-first ingestion boundary
- card retrieval changes must preserve citation and audit semantics
- runtime adoption work must include rollback criteria for agent behavior changes

The first Phase-3 implementation should be selected only after one candidate has
enough evidence to justify implementation. Until then, Phase 3 remains an intake
queue, not an implementation commitment.

## Phase 3 Candidate Evidence Audit

P53 Phase-3 candidate evidence audit records current readiness. No Phase-3 candidate is ready for direct implementation yet.

Candidate readiness:

- Runtime adoption evidence: recommended first measurement track. It should collect concrete agent-behavior evidence before default policy changes.
- Card retrieval maturity: partial evidence from P43-P45, but it still needs measured retrieval misses and context impact before default context changes or card embeddings.
- Evaluator APIs: blocked on advisory output contracts and lifecycle replay requirements.
- Research adapter ingestion: blocked on an explicit external report/input contract.

Recommended first Phase-3 track: runtime adoption evidence. Runtime adoption evidence is the common measurement substrate for deciding whether card-aware context should become default, whether card embeddings are justified, and what evaluator advice is actually useful to agents.

This keeps Phase 3 evidence-first: implement measurement before implementing
new authority, new retrieval defaults, or new external ingestion adapters.

## Phase 3 Runtime Surfaces

P54 runtime adoption evidence adds schema v9 table `runtime_adoption_events`.
Events capture explicit agent/runtime signals with `track`, `signal`, `feature`,
optional query/context/card/evaluator/research references, note, metadata, and
timestamp.

P55 runtime adoption CLI exposes this evidence substrate:

- `mempal phase3 adoption record`
- `mempal phase3 adoption list`
- `mempal phase3 adoption stats`

P56 implements `mempal phase3 gate card-context-default`. Card context default gate remains read-only; `include_cards` remains opt-in. The gate requires
accepted `card_context` adoption evidence and zero rollback signals before a
future default-on spec can even be considered.

P57 implements `mempal phase3 gate card-embeddings`. The gate remains read-only
and adds no card vector schema. Card embeddings require repeated measured
`card_embedding` miss signals, and linked evidence remains the citation root.

P58 implements `mempal phase3 gate evaluator-api`. Evaluator API gate remains advisory-only and preserves the P50 advisory-only lifecycle boundary: evaluator
signals cannot mutate status, satisfy reviewer requirements, or bypass
deterministic gates.

P59 implements `mempal phase3 research-validate-plan`. The command validates an
external JSON report/input contract with `report_id`, `title`, `sources`,
`findings`, and optional `candidate_insights`. It only validates and plans;
research adapter ingestion still preserves the P49 evidence-first boundary and
does not create promoted/canonical knowledge.

P60 exposes the Phase-3 runtime evidence baseline to MCP-connected agents
through `mempal_phase3`. The MCP tool uses `action` values
`record/list/stats/gate/research_validate_plan`, mirroring the P54-P59 CLI
surfaces without adding new authority. Later Phase-3 actions extend this same
bounded MCP surface. `record` appends `runtime_adoption_events`; `list`,
`stats`, `gate`, and `research_validate_plan` remain read-only. MCP research
validation accepts a JSON report object and still does not ingest or promote
knowledge.

P61 adds a read-only runtime adoption recording protocol through
`mempal_phase3 action=guidance`. The guidance tells agents when to record
`used`, `accepted`, `rejected`, `miss`, `rollback`, `contradiction`, or
`neutral`, and exposes the required `track`, `signal`, and `feature` fields
plus optional context fields. This is a recording discipline, not automatic
instrumentation: it adds no hooks, no background writes, no schema migration,
and no default runtime behavior change.

P62 exposes the same recording protocol through CLI parity with
`mempal phase3 adoption guidance`. The CLI supports plain and JSON output and
shares the guidance implementation with `mempal_phase3 action=guidance`, so MCP
agents, humans, and non-MCP automation inspect the same semantics. This remains
read-only and does not append adoption events or change Phase-3 gate policy.

P63 adds a read-only record helper through
`mempal phase3 adoption prepare-record` and
`mempal_phase3 action=prepare_record`. The helper validates and normalizes
candidate record inputs, then returns the equivalent CLI `record` command and
MCP `record` payload with `writes=false`. It does not generate event ids unless
the caller supplied one, and it does not append runtime adoption events.

P64 adds a read-only record quality policy through
`mempal phase3 adoption check-record` and
`mempal_phase3 action=check_record`. The policy evaluates candidate runtime
adoption event quality before writing and returns `writes=false`, `valid`,
`quality`, `errors`, and `warnings`. It treats empty `feature` as an error,
warns when outcome-bearing signals lack concrete note/query context, and warns
when track-specific references such as `card_id`, `evaluator_id`, or
`research_report_id` are missing. This remains advisory only: it does not append
events and does not block the lower-level `record` command.

P65 adds a read-only runtime adoption review report through
`mempal phase3 adoption review` and `mempal_phase3 action=review`. The report
summarizes accumulated adoption evidence with applied filters, aggregate signal
counts, per-feature counts, an advisory conclusion, and reasons. It supports
track, feature, and signal filtering without schema changes; signal filtering is
applied after DB retrieval. This gives future default-on specs a compact
evidence artifact while preserving the Phase-3 boundary: review reports do not
write events, change gates, or authorize runtime default changes.

P66 adds a read-only card-context default readiness report through
`mempal phase3 readiness card-context-default` and
`mempal_phase3 action=readiness` with `candidate=card-context-default`. The
report reuses P65 review semantics filtered to `track=card_context` and
`feature=include_cards`, then returns `writes=false`, `ready`, `decision`, the
embedded review, and reasons. `ready=true` only means the surface is eligible
for a future default-on spec; it does not change `mempal context` defaults,
enable `include_cards`, mutate lifecycle state, or create card embeddings.

P67 adds explicit evidence-first research ingestion through
`mempal phase3 research-ingest-plan`. The command accepts the same P59 report
contract and defaults to dry-run with `writes=false`. With `--execute`, it
writes one `memory_kind=evidence` drawer per finding using
`provenance=research`, stable drawer ids, and idempotent skip-on-existing
behavior. `candidate_insights` are surfaced only as `mempal knowledge distill`
suggestions backed by the planned evidence refs; P67 does not create knowledge
drawers, promote research output, add a schema migration, or expose MCP write
access.

P68 exposes the P67 dry-run planning semantics through MCP as
`mempal_phase3 action=research_ingest_plan`. The action accepts an inline
`report` JSON object, returns planned research evidence drawer refs plus
candidate `mempal knowledge distill` suggestions, and always reports
`writes=false`. It shares the same pure planner as the CLI but deliberately does
not expose `--execute`, create drawers or vectors, mutate runtime adoption
events, or promote research output into knowledge.

P69 adds a quality-gated runtime adoption write path through
`mempal phase3 adoption record-checked` and
`mempal_phase3 action=record_checked`. The command/action runs the P64 record
quality policy immediately before writing. `quality=ready` records are written,
`quality=warning` records are blocked by default unless `allow_warnings` is
explicitly set, and `quality=invalid` records are always blocked. This reduces
low-signal self-evolution evidence without adding hooks, background
instrumentation, schema changes, or new authority for Phase-3 gates.

P70 self-evolution completion audit records the current state against the
larger objective: a complete self-evolving agent system. Complete
self-evolving agent system deliverables are:

- evidence substrate: the system can store cited raw evidence and runtime
  adoption outcomes without losing provenance
- knowledge governance: evidence can be distilled into governed knowledge and
  moved through lifecycle gates
- runtime retrieval: agents can request context/search/card/research guidance
  without changing defaults implicitly
- research bridge: external research output can enter as evidence and candidate
  insight suggestions, not as direct dao
- feedback loop: runtime use, acceptance, rejection, misses, rollbacks, and
  contradictions can be recorded and reviewed
- policy hardening path: stronger defaults require evidence, readiness checks,
  rollback criteria, and a new P-level spec

Prompt-to-artifact checklist:

- Evidence substrate -> P54 runtime_adoption_events plus P0-P13 raw drawer
  storage and cited search provide durable evidence records.
- Knowledge governance -> P12-P28 implement typed `dao_tian` / `dao_ren` /
  `shu` / `qi` drawers, context assembly, policy surfaces, distill, gate,
  promote, demote, and anchor publication.
- Knowledge cards -> P31-P45 implement Phase-2 card schema, core API, CLI,
  MCP read/lifecycle/retrieval, backfill, and explicit card-aware context.
- Research ingestion -> P49/P59/P67/P68 preserve evidence-first research
  boundaries: validate report, plan evidence refs, write research evidence only
  through explicit CLI `--execute`, and expose MCP dry-run planning.
- Runtime adoption -> P54-P69 implement event storage, CLI/MCP record/list/stats,
  guidance, prepare/check helpers, review, readiness, and quality-gated
  `record_checked` writes.
- Default hardening -> P56/P57/P58/P66 define read-only gates and readiness
  reports for card context, card embeddings, evaluator APIs, and
  card-context-default eligibility.

P70 conclusion: not complete. P12-P69 establish a governed self-evolution
substrate, but they do not yet prove a complete self-evolving agent system.
Remaining gaps before full self-evolution:

- no automatic or semi-automatic adoption capture around actual agent tool
  execution; evidence still depends on explicit CLI/MCP calls
- no end-to-end replay that demonstrates research -> evidence -> distill ->
  gated promotion -> runtime context -> checked adoption record in one audited
  scenario
- no evaluator advisory API with replayable output contracts; P50/P58 only keep
  evaluators advisory and gated
- no default-on card context or card embeddings; P66 readiness only reports
  eligibility for a future default-on spec
- no autonomous promotion authority; lifecycle changes still require deterministic
  gates, evidence refs, and human/reviewer boundaries
- no rollback executor for default/runtime policy changes; rollback criteria are
  policy requirements, not an automated runtime mechanism

Future P candidates:

- P71 self-evolution loop replay: implemented as a CLI E2E replay test that
  walks research -> evidence -> knowledge card -> gate/promote -> context ->
  checked adoption record. This proves the existing pieces can compose, but it
  does not add automatic runtime capture.
- P72 runtime adoption capture helper: implemented as explicit CLI/MCP
  `capture` surfaces that map `surface/outcome` observations into existing
  checked runtime adoption records. It is dry-run by default, writes only with
  explicit execute, and does not add background instrumentation.
- P73 evaluator advisory API: implemented as deterministic CLI/MCP advice
  surfaces through `mempal phase3 evaluator advise` and
  `mempal_phase3 action=evaluator_advise`. Advice output is replayable from
  request fields, returns `writes=false`, `lifecycle_authority=false`,
  `deterministic_gate_required=true`, reasons, and a `surface=evaluator`
  adoption capture plan. It cannot mutate lifecycle state, satisfy reviewer
  requirements, bypass gates, or call LLM/network evaluators.
- P74 card context default-on proposal: implemented as read-only CLI/MCP
  proposal surfaces through `mempal phase3 default-proposal card-context` and
  `mempal_phase3 action=default_proposal`. The proposal embeds P66 readiness,
  requires explicit rollback criteria, returns `writes=false`, and only marks
  `proposal_ready=true` when both readiness and rollback criteria are present.
  It deliberately keeps `mempal context` / `mempal_context` default
  `include_cards=false`; any actual default change still requires a future
  explicit spec.

P75 self-evolution completion audit revisits the full objective after P71-P74
landed on main.

P75 objective restatement:

- The system must preserve raw evidence and runtime outcome evidence with
  provenance.
- The system must distill evidence into governed knowledge and cards through
  deterministic gates.
- The system must expose retrieval/context surfaces that agents can use without
  silently changing runtime defaults.
- The system must accept external research only through evidence-first and
  evidence-backed candidate insight paths.
- The system must record, review, and quality-gate runtime adoption feedback.
- The system must provide evaluator advice without granting lifecycle authority.
- The system must provide a policy-hardening path where stronger defaults require
  evidence, readiness checks, rollback criteria, and a new explicit spec.

P75 prompt-to-artifact checklist:

- Evidence substrate: P0-P13 raw drawer storage and citation-bearing search,
  P54 `runtime_adoption_events`, and schema v9 tests prove durable evidence and
  runtime outcome storage.
- Knowledge governance: P12-P28 typed `dao_tian` / `dao_ren` / `shu` / `qi`
  drawers, policy surfaces, distill, gate, promote/demote, and anchor
  publication remain covered by `tests/knowledge_lifecycle.rs`.
- Knowledge cards: P31-P45 card schema, core API, CLI, MCP, retrieval, backfill,
  and lifecycle surfaces remain covered by `tests/knowledge_card_*` and explicit
  card-aware context tests.
- Research bridge: P49/P59/P67/P68 ensure research output enters as evidence or
  evidence-backed candidate insight suggestions; P71 proves this path in
  `tests/phase3_self_evolution_replay.rs`.
- Self-evolution replay: P71 `tests/phase3_self_evolution_replay.rs` walks
  research -> evidence -> card promotion -> context -> checked adoption record.
- Adoption capture: P72 `mempal phase3 adoption capture` and
  `mempal_phase3 action=capture` map concrete `surface/outcome` observations
  into checked records without background instrumentation.
- Evaluator advice: P73 `mempal phase3 evaluator advise` and
  `mempal_phase3 action=evaluator_advise` return replayable advisory output with
  `lifecycle_authority=false` and `deterministic_gate_required=true`.
- Default hardening proposal: P74 `mempal phase3 default-proposal card-context`
  and `mempal_phase3 action=default_proposal` combine P66 readiness with
  rollback criteria while preserving `include_cards=false`.
- Protocol and inventory evidence: `src/core/protocol.rs`, `AGENTS.md`, and
  `CLAUDE.md` list the Phase-3 actions through
  `capture/evaluator_advise/default_proposal`.
- Mainline verification evidence: PRs #63, #64, #65, and #66 were merged to
  main with green `fmt`, `default`, and `rest` CI checks.

P75 conclusion: not complete.

The governed self-evolution substrate is now substantially complete: evidence,
knowledge governance, cards, research ingestion, runtime adoption feedback,
replay, capture helpers, evaluator advice, and default-on proposal artifacts are
implemented and tested. However, the full "complete self-evolving agent system"
objective still has uncovered requirements if interpreted as autonomous runtime
self-evolution.

Remaining gaps after P75:

- no automatic live tool instrumentation: adoption capture still requires
  explicit CLI/MCP calls rather than wrapping actual agent tool execution
- no actual default-on runtime change: `include_cards` remains opt-in by design,
  and P74 only creates a proposal artifact
- no rollback executor: rollback criteria are recorded in proposals but are not
  executable runtime policy
- no autonomous promotion authority: lifecycle mutation still requires
  deterministic gates, evidence refs, and human/reviewer boundaries
- no card embedding implementation: P57/P47 keep card-level embeddings behind
  measured miss evidence and future rollback requirements

P76 spec completeness invariant records the process rule that every numbered P
must leave both a task contract and a matching plan. This includes
documentation-only, audit-only, policy-only, and code implementation work. The
rule exists because the P-series is no longer just a task list; it is the
auditable decision trail for the mind-model implementation. Every P must leave
a spec before it can be considered complete. A future spec-less P or missing
spec is explicitly incomplete and must be fixed before implementation or merge.

Updated recommended next P candidates after reserving P76 for governance:

P77 live adoption instrumentation boundary adds a read-only policy surface for
the live instrumentation gap. `mempal phase3 adoption instrumentation-policy`
and `mempal_phase3 action=instrumentation_policy` return `writes=false`,
`default_mode=manual_only`, allow only `opt_in_wrapper` as the semi-automatic
mode, and explicitly forbid `implicit_background_capture`, silent event append,
and quality gate bypass. This does not install hooks or wrappers; it defines the
safe boundary future instrumentation must obey: opt-in, user opt-out, checked
capture/record_checked writes, and rollback evidence when instrumentation
degrades behavior.

Updated recommended next P candidates after completing P77:

P78 card context default runtime flag implements the first actual default
runtime change path for cards. The default remains `false` unless local config
sets `context.include_cards_default=true`. `mempal context` and `mempal_context`
use that config only when the request omits explicit card flags; CLI
`--include-cards` still opts in, CLI `--no-include-cards` opts out, and MCP
`include_cards` overrides config when supplied. The only supported write path is
`mempal phase3 default-control card-context`: enabling requires the P74
proposal-ready conditions, including sufficient `card_context/include_cards`
runtime adoption evidence and rollback criteria; disabling is always allowed
and writes the flag back to false. The command writes local config only and does
not append runtime adoption events, change schema, or alter search defaults.

Updated recommended next P candidates after completing P78:

- P79 rollback executor policy implements the first concrete rollback executor
  for default/runtime policy changes. CLI `mempal phase3 rollback-control
  card-context` evaluates `card_context/include_cards` rollback evidence and,
  only with `--execute`, writes local config
  `context.include_cards_default=false`. MCP `mempal_phase3
  action=rollback_control` exposes the same rollback evidence check as a
  read-only agent surface. No runtime adoption events, knowledge lifecycle
  state, schema, or search defaults are changed by rollback control.

Updated recommended next P candidates after completing P79:

P80 autonomous promotion boundary audit resolves the last ambiguous "gap" from
P70/P75 as a governance boundary rather than a missing implementation.
Autonomous promotion is out of scope for the current complete self-evolution
design. mempal can autonomously preserve evidence, prepare candidate knowledge,
evaluate gates, produce evaluator advice, assemble context, propose default
changes, and execute explicit rollback controls, but lifecycle authority remains
human-gated.
P80 decision: autonomous promotion is out of scope.

human-gated lifecycle authority is the final governance boundary: promotion and
demotion of Stage-1 knowledge drawers or Phase-2 knowledge cards must remain
explicit human/operator-triggered lifecycle mutation surfaces. Deterministic
gates, evidence refs, reviewer rules, evaluator advice, runtime adoption
evidence, and research findings can support the decision, but none of them can
silently convert a candidate into promoted knowledge. This boundary keeps the
system self-evolving in the evidence/proposal/context/adoption loop while
avoiding an agent that can grant itself durable knowledge authority.

Updated recommended next P candidate after completing P80:

- P81 self-evolution completion audit: re-evaluate the active objective against
  the actual artifacts after P77-P80. If the governed, human-gated definition is
  accepted as the intended objective, the audit can close the goal; if the goal
  still requires fully autonomous lifecycle mutation, that requirement must be
  reopened as a separate explicit spec rather than inferred.

P81 self-evolution completion audit is the final audit for the active objective
`完整自进化 agent 系统`.

Objective restatement: the target is a governed human-gated complete
self-evolving agent system. "Complete" means the agent can gather external and
runtime evidence, preserve provenance, distill and structure knowledge, retrieve
the right knowledge/skills/tools at runtime, record feedback, evaluate stronger
defaults, apply explicit default/rollback controls, and keep durable knowledge
lifecycle mutation under deterministic gates plus human/operator intent.

Prompt-to-artifact checklist:

- Evidence substrate: P0-P13 raw drawer storage, citation-bearing search, and
  P54 `runtime_adoption_events` provide durable evidence and runtime outcome
  storage. Evidence remains raw and cited; search/context do not rewrite source
  content.
- Knowledge governance: P12-P28 typed `dao_tian` / `dao_ren` / `shu` / `qi`
  drawers, policy surfaces, distill, gate, promote/demote, and anchor
  publication provide governed knowledge lifecycle for Stage-1 drawers.
- Knowledge cards: P31-P45 implement card schema, evidence links, append-only
  events, CLI/MCP lifecycle, backfill, retrieval, and explicit card-aware
  context without making cards an implicit search/default source.
- Research bridge: P49/P59/P67/P68 ensure external research enters as evidence
  or evidence-backed candidate insight suggestions. Research output cannot
  directly define dao or bypass gates.
- Runtime feedback loop: P54-P69 provide runtime adoption event storage,
  guidance, prepare/check helpers, review/readiness/gate reports, and
  quality-gated `record_checked` writes.
- Self-evolution replay: P71 `tests/phase3_self_evolution_replay.rs` proves the
  composed path research -> evidence -> card promotion -> context -> checked
  adoption record.
- Live adoption boundary: P77 `instrumentation_policy` defines the safe boundary
  for future live wrappers: opt-in, preserve opt-out, no silent event append,
  and route writes through checked capture or `record_checked`.
- Runtime default control: P74/P78 provide proposal-ready and explicit
  `default-control` paths for card-aware context. Default change requires
  runtime evidence and rollback criteria; request-level overrides still win.
- Rollback and default control: P79 `rollback-control` turns rollback evidence
  into an explicit reversible config action, setting
  `context.include_cards_default=false` only with `--execute` and without
  writing runtime events or lifecycle state.
- Evaluator boundary: P50/P58/P73 keep evaluators advisory-only.
  `evaluator_advise` is replayable, returns `lifecycle_authority=false`, and
  cannot satisfy reviewer authority or bypass deterministic gates.
- Lifecycle authority boundary: P80 declares autonomous promotion out of scope.
  Human/operator-triggered promote/demote commands with evidence refs and gates
  remain the only durable lifecycle mutation path.
- Spec completeness: P76 requires every numbered P to leave a matching task spec
  and plan. P77-P81 follow this rule.
- Mainline verification: PR #68 through PR #72 are merged to main. Main CI runs
  `25805677837`, `25806999068`, `25808402185`, `25809830828`, and
  `25810588996` all completed with success across `fmt`, `default`, and `rest`
  jobs.

P81 conclusion: complete.

The active objective is complete under the governed human-gated definition. The
system now has an auditable loop from evidence intake to governed knowledge,
runtime context, feedback capture, policy evaluation, explicit default control,
and explicit rollback. It does not grant agents silent durable lifecycle
authority, and that is an intentional design boundary rather than a missing
implementation.

Residual boundary: fully autonomous lifecycle mutation remains out of scope. If
future work requires an agent to promote or demote durable knowledge without a
human/operator-triggered lifecycle command, that is a new objective and must be
opened as a separate P-level spec with its own evidence, rollback, safety, and
acceptance criteria.

## Post-P81 Opt-In Instrumentation

P82 implements the first concrete `opt_in_wrapper` allowed by P77. CLI
`mempal phase3 adoption wrap` explicitly runs one child command after `--`,
observes its exit status, maps `0` to `accepted` and non-zero to `rejected`
unless `--outcome` overrides the mapping, and returns a wrapper report with the
child exit code plus the nested capture report.

P82 preserves the governed runtime boundary:

- no hooks, daemons, background workers, or silent capture are installed
- no MCP-side shell command execution is added
- no runtime adoption event is written unless `--execute` is supplied
- all writes reuse P72 capture mapping and P69 checked-record quality gates
- warning-quality captures remain blocked unless `--allow-warnings` is supplied
- non-zero child exits are propagated after the wrapper report is emitted

This makes runtime evidence capture less manual while keeping instrumentation
explicit, opt-in, quality-gated, and reversible through existing rollback
evidence policies.

## Cognitive Brief

P83 adds `mempal brief`, the first deterministic cognitive brief surface. It
does not replace `mempal search` or `mempal context`; it uses the existing
context assembly path with evidence and cards enabled, then organizes the result
into a citation-first report.

The P83 brief contains:

- key facts from governed knowledge/context items
- cited evidence items
- active knowledge cards and their linked evidence citations
- simple entity cues extracted from cited text
- unresolved-item cues such as action items, blockers, or remaining work
- uncertainty signals such as missing evidence, missing governed knowledge,
  missing cards, unresolved work, or conflict/stale language
- deterministic next actions

P83 deliberately avoids LLM synthesis, MCP command execution, schema changes,
fact-check side effects, dream-cycle maintenance, or runtime adoption writes.
Its purpose is to make the system "read for you" in a safe first step: organize
cited memory into an actionable brief while preserving uncertainty instead of
hallucinating confidence.

## Multi-Agent Cowork Bus

P84 upgrades cowork from a two-tool Claude/Codex pair protocol into a
project-scoped multi-agent bus. The old P8 path routes to tool-family inboxes
such as `claude` or `codex`; that is insufficient when one project has one
Claude Code instance and multiple Codex instances, because both Codex instances
race on the same shared `codex` inbox.

The P84 bus introduces stable `agent_id` addressing:

- `cowork-register` records concrete instances such as `claude-main`,
  `codex-a`, and `codex-b`
- `cowork-send` targets one concrete `agent_id`
- `cowork-broadcast` fans out independent inbox copies to multiple targets
- `cowork-agent-drain` drains one concrete agent inbox
- `cowork-agents` lists registered agents and pending per-agent inbox state

State lives under `~/.mempal/cowork-bus/<encoded_project_identity>/`, outside
`palace.db`. P84 remains ephemeral and file-backed: it does not write drawers,
cards, runtime adoption events, audit entries, or schema state. The legacy
`cowork-drain --target claude|codex` and `mempal_cowork_push` path remains
available unchanged for backward compatibility.

P84 stores optional transport metadata but only inbox delivery is active. tmux
send/capture is intentionally left to the next P-level task so that instance
identity and per-agent routing are proven before adding a more invasive
terminal-injection transport.

## MCP Multi-Agent Cowork Bus

P85 exposes the P84 bus to agent runtimes through one MCP tool:
`mempal_cowork_bus`. This is the point where the bus becomes usable by agents
directly, not only by shell commands.

The MCP surface is action-based:

- `action=register` registers or updates a concrete `agent_id`
- `action=list` reports project bus agents and pending inbox counts
- `action=send` delivers one message to one concrete target
- `action=broadcast` fans out independent inbox copies to multiple targets
- `action=drain` returns and consumes one concrete agent's inbox

P85 deliberately does not infer concrete instances from MCP `client_info.name`.
Client names can identify a tool family such as Codex, but they cannot
distinguish `codex-a` from `codex-b`. The multi-agent bus therefore requires
explicit `agent_id` values and remains separate from legacy
`mempal_cowork_push`, which is still the simpler Claude<->Codex partner
handoff path.

The MCP tool uses the same file-backed bus state under
`~/.mempal/cowork-bus/<encoded_project_identity>/`. It does not write
`palace.db`, drawers, cards, runtime adoption events, or schema state. tmux
transport remains a later layer on top of the now-explicit agent registry and
per-agent routing model.

## Tmux Cowork Transport

P86 activates tmux as an explicit transport for the multi-agent bus. `inbox`
remains the default safe path. A target agent can opt into near-real-time pane
delivery by registering with `transport=tmux` and a concrete `tmux_target`.

Example:

```bash
mempal cowork-register \
  --agent-id codex-a \
  --tool codex \
  --cwd "$PWD" \
  --transport tmux \
  --tmux-target mempal:0.1
```

After that, `cowork-send`, `cowork-broadcast`, and
`mempal_cowork_bus action=send|broadcast` use the same transport-aware bus core.
For tmux targets, mempal invokes the local `tmux` binary directly with
`std::process::Command`; it does not execute through a shell. The delivered text
is a plain envelope containing source agent id, target agent id, and message
content.

P86 intentionally does not silently fall back to inbox if tmux fails. A tmux
transport target means "deliver to this pane"; if that pane or binary is
unavailable, the send fails and no inbox copy is written. This prevents
ambiguous double-delivery semantics where a pane may receive a message and then
later drain the same message from an inbox.

This gives mempal three cowork layers:

- legacy partner handoff: `mempal_cowork_push` for Claude<->Codex pairs
- multi-agent bus inbox: explicit `agent_id` routing with per-agent inbox files
- tmux transport: explicit pane delivery for registered concrete agents

## Cowork Bus Event Log

P87 adds an operational event log to the multi-agent bus. Communication is no
longer only "fire and inspect inbox"; every important bus action also appends a
JSON Lines event under:

```text
~/.mempal/cowork-bus/<encoded_project_identity>/events.jsonl
```

The event stream records:

- `register` events when a concrete agent id is registered or updated
- `send` and `broadcast` delivery events for successful inbox or tmux delivery
- `send` / `broadcast` failure events for tmux hard failures
- `drain` events when an agent drains its per-agent inbox

Replay is intentionally read-only. `mempal cowork-events --cwd <repo>` and
`mempal_cowork_bus action=events` list the operational event stream; they do
not redeliver messages, drain inboxes, trigger tmux, or ingest anything into
`palace.db`. Message bodies are represented as bounded `message_preview`
fields, so the event stream is an operational audit trail rather than a second
durable memory store.

This is the first runtime-ops layer above P84-P86. It gives later delivery
ack/status, presence, thread/channel, and tmux peek work a shared evidence
source for "what happened on the bus" without changing the core memory schema.

## Cowork Delivery Ack And Status

P88 derives delivery status from the P87 event stream. There is no mutable
status table and no database migration. The event id of each successful or
failed delivery is the `message_id` surfaced to CLI and MCP callers.

Status is computed by replaying `events.jsonl`:

- `pending`: delivery succeeded and has not been drained or acked
- `drained`: a later drain event consumed the target agent's inbox message
- `acked`: the target agent explicitly appended an ack event for that
  `message_id`
- `failed`: the original delivery event was a hard transport failure

The user-facing surfaces are:

```bash
mempal cowork-deliveries --cwd "$PWD" --agent-id codex-a
mempal cowork-ack --cwd "$PWD" --agent-id codex-a --message-id evt-...
```

The MCP surface reuses `mempal_cowork_bus` with `action=deliveries` and
`action=ack`. Ack is explicit and append-only: it does not mutate inbox files,
does not redeliver messages, and does not write `palace.db`. This keeps the
bus operationally observable while preserving the original ephemeral cowork
boundary.

## Cowork Agent Presence

P89 adds explicit heartbeat-based presence. Registration gives each concrete
agent a `last_seen_at`, and agents can refresh it with:

```bash
mempal cowork-heartbeat --cwd "$PWD" --agent-id codex-a
```

Presence is derived when listing agents:

- `online`: last seen within the default 10 minute stale threshold
- `stale`: last seen exists but is older than the stale threshold
- `never_seen`: legacy or hand-edited records without `last_seen_at`

The same semantics are exposed through `mempal_cowork_bus action=heartbeat`
and `action=list`. This remains an explicit signal: mempal does not install a
daemon, does not infer liveness from tmux panes, and does not silently record
background heartbeat events. That keeps presence useful for coordination
without pretending to know more than the agent instances have explicitly
reported.

## Cowork Threads And Channels

P90 adds two coordination scopes above raw agent addressing:

- `thread_id` separates work streams such as `p90-review` or `release-audit`
- `channel` names a group of concrete agents such as `review` or `frontend`

Direct `cowork-send` and `cowork-broadcast` can attach `thread_id` and
`channel` metadata. The metadata is carried into bus inbox messages, events,
and delivery status replay, so a receiver can see which work stream a message
belongs to when draining its inbox.

Channels are explicit registry entries, not inferred from tool family names:

```bash
mempal cowork-channel-set \
  --cwd "$PWD" \
  --channel review \
  --agent codex-a \
  --agent codex-b

mempal cowork-channel-send \
  --cwd "$PWD" \
  --from claude-main \
  --channel review \
  --thread-id p90-review \
  --message "review this patch"
```

`cowork-channel-set` replaces membership for one channel, and
`cowork-channel-send` fans out through the same delivery core as broadcast. The
MCP surface exposes the same behavior as `mempal_cowork_bus`
`action=channel_set|channel_list|channel_send`.

P90 still does not make channels durable memory. They are operational routing
state under `~/.mempal/cowork-bus/<project>/`, and they do not write
`palace.db`.

## Cowork Tmux Live Peek

P91 completes the tmux runtime-ops loop by adding explicit read-only pane
inspection for agents registered with `transport=tmux` and a concrete
`tmux_target`.

```bash
mempal cowork-tmux-peek \
  --cwd "$PWD" \
  --agent-id codex-a \
  --lines 80
```

The MCP surface is `mempal_cowork_bus action=tmux_peek`. Both CLI and MCP use
the same adapter: a direct `std::process::Command` invocation of the local
`tmux` binary with `capture-pane`. It is not executed through a shell, and it
does not discover panes automatically. The registered `tmux_target` is the
authority.

Peek is deliberately not delivery. It does not append `events.jsonl`, does not
write an inbox message, does not update channel or agent registry state, and
does not write `palace.db`. Capture failure is a hard error rather than a
fallback to inbox or legacy `mempal_peek_partner`.

This preserves the separation between:

- agent live observation: read-only tmux pane capture
- operational communication: inbox/tmux send through the cowork bus
- durable memory: explicit ingest into the palace database

## Multi-Agent Cowork Runbook

P92 makes the multi-agent runtime surfaces operationally usable by adding the
authoritative [COWORK-RUNBOOK](COWORK-RUNBOOK.md) plus a read-only CLI surface:

```bash
mempal cowork-runbook --format plain
mempal cowork-runbook --format json
```

The runbook describes concrete agent registration, direct send, broadcast,
channels, threads, drain, delivery status, ack, presence, tmux delivery, tmux
peek, doctor, sessions, handoff summaries, and explicit memory capture. Reading
the runbook does not touch `~/.mempal` or `palace.db`.

## Cowork Doctor

P93 adds deterministic runtime diagnostics:

```bash
mempal cowork-doctor --cwd "$PWD"
mempal cowork-doctor --cwd "$PWD" --probe-tmux --format json
```

The MCP equivalent is `mempal_cowork_bus action=doctor`. Doctor checks registry
size, channel count, session count, stale or never-seen agents, pending
deliveries, and optional tmux target reachability. tmux probing uses direct
`tmux has-session -t <target>` invocation, not a shell.

Doctor is read-only. It does not append events, drain inboxes, update
heartbeats, repair state, or write memory.

## Cowork Team Sessions

P94 adds runtime team sessions stored under:

```text
~/.mempal/cowork-bus/<encoded_project_identity>/sessions.json
```

Sessions bind a collaboration goal to concrete agents, optional channels, and
an optional thread id:

```bash
mempal cowork-session-create \
  --cwd "$PWD" \
  --session-id p94-review \
  --title "P94 review" \
  --agent claude-main \
  --agent codex-a \
  --thread-id p94-review

mempal cowork-sessions --cwd "$PWD"
mempal cowork-session-status --cwd "$PWD" --session-id p94-review --status paused
```

The MCP actions are `session_create`, `session_list`, and `session_status`.
Session changes append operational events, but they do not become durable
project memory and they do not change message delivery semantics.

## Cowork Handoff Summary

P95 adds deterministic handoff summaries:

```bash
mempal cowork-handoff --cwd "$PWD"
mempal cowork-handoff --cwd "$PWD" --thread-id p95-review --format json
```

The MCP action is `mempal_cowork_bus action=handoff`. A handoff summarizes
active sessions, agents and presence, pending deliveries, and recent events. It
supports `thread_id`, `channel`, `session_id`, and `limit` filters. It does not
call an LLM, drain inboxes, ack deliveries, or persist memory.

## Cowork Memory Capture

P96 adds the explicit bridge from runtime cowork state to durable evidence:

```bash
mempal cowork-capture \
  --cwd "$PWD" \
  --summary-source handoff \
  --session-id p95-review \
  --execute \
  --format json
```

The MCP action is `mempal_cowork_bus action=capture`. Capture defaults to
dry-run. With `--execute` / `execute=true`, it writes one evidence drawer under
wing `cowork-capture` by default. It does not capture raw tmux pane text, does
not promote knowledge, does not create knowledge cards, and does not alter
delivery status. This keeps runtime communication ephemeral unless an agent or
human explicitly crosses the memory boundary.

## Maintenance Runbook

P97 adds the authoritative [MAINTENANCE-RUNBOOK](MAINTENANCE-RUNBOOK.md) plus a
read-only CLI:

```bash
mempal maintenance-runbook --format plain
mempal maintenance-runbook --format json
```

The runbook stitches together research validation, research evidence ingest,
knowledge distill, card lifecycle gates, context adoption, runtime adoption
review, rollback, cowork handoff, and explicit cowork capture. It is a
checklist for dream-cycle style maintenance, not a daemon or scheduler.

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
