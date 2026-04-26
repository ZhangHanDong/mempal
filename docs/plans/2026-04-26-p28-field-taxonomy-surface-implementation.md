# P28 Field Taxonomy Surface Implementation Plan

**Goal:** Add a read-only field taxonomy guidance surface so agents can choose stable Stage-1 `field` values without overloading wing/room taxonomy or enforcing a schema.

**Architecture:** Add a static `field_taxonomy` module with recommended entries. Wire it to a CLI command and MCP tool. Keep all existing ingest/search/context behavior permissive for custom fields.

## Steps

- [x] Validate task contract with `agent-spec parse/lint`.
- [x] Add shared field taxonomy entry types and static recommended entries.
- [x] Add `mempal field-taxonomy --format plain|json`.
- [x] Add `mempal_field_taxonomy` MCP tool and DTO.
- [x] Add CLI, MCP, and custom-field permissiveness tests.
- [x] Update usage docs, MIND-MODEL-DESIGN, AGENTS, and CLAUDE.
- [x] Run formatting, checks, clippy, and tests.

## Verification

```bash
agent-spec parse specs/p28-field-taxonomy-surface.spec.md
agent-spec lint specs/p28-field-taxonomy-surface.spec.md --min-score 0.7
cargo fmt --check
cargo check
cargo check --features rest
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```
