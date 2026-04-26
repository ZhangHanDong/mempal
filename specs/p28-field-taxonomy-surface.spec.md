spec: task
name: "P28: read-only field taxonomy guidance surface"
inherits: project
tags: [memory, mind-model, field, taxonomy, mcp, cli]
---

## Intent

`field` is already an explicit Stage-1 drawer metadata dimension used by search
and context filtering, but it has no management guidance. Existing
`mempal taxonomy` manages wing/room routing keywords and should not be overloaded
with mind-model `field` semantics.

This task adds a read-only field taxonomy guidance surface. It gives agents and
humans a stable list of recommended Stage-1 fields without enforcing those names
or changing storage.

## Decisions

- Add a read-only CLI command `mempal field-taxonomy`.
- CLI supports `--format plain|json`, defaulting to `plain`.
- Add MCP tool `mempal_field_taxonomy`.
- The response lists recommended entries with:
  - `field`
  - `domains`
  - `description`
  - `examples`
- Include at least these Stage-1 fields:
  - `general`
  - `epistemics`
  - `software-engineering`
  - `debugging`
  - `tooling`
  - `research`
  - `writing`
  - `diary`
- The field taxonomy is guidance only; ingest, distill, search, and context must
  continue accepting custom field strings.
- Do not reuse the existing `taxonomy` SQLite table; that table remains
  wing/room routing taxonomy only.
- Do not change database schema.

## Acceptance Criteria

Scenario: CLI prints field taxonomy as JSON
  Test:
    Package: mempal
    Filter: test_cli_field_taxonomy_json_lists_stage1_fields
  Given any initialized mempal home
  When running `mempal field-taxonomy --format json`
  Then the command exits successfully
  And JSON includes `general`, `epistemics`, `software-engineering`, `tooling`, and `diary`
  And the `epistemics` entry includes domain `global`

Scenario: CLI prints field taxonomy as plain text
  Test:
    Package: mempal
    Filter: test_cli_field_taxonomy_plain_lists_descriptions
  Given any initialized mempal home
  When running `mempal field-taxonomy`
  Then stdout includes `epistemics`
  And stdout includes `domains=`

Scenario: MCP exposes field taxonomy guidance
  Test:
    Package: mempal
    Filter: test_mcp_field_taxonomy_lists_stage1_fields
  Given an MCP server
  When calling `mempal_field_taxonomy`
  Then the response includes the same Stage-1 field entries

Scenario: field taxonomy does not restrict custom fields
  Test:
    Package: mempal
    Filter: test_field_taxonomy_does_not_restrict_custom_context_field
  Given a knowledge drawer with custom `field="compiler-design"`
  When assembling context with `field="compiler-design"`
  Then the custom-field drawer is still returned

Scenario: CLI rejects unsupported field taxonomy format
  Test:
    Package: mempal
    Filter: test_cli_field_taxonomy_rejects_invalid_format
  Given any initialized mempal home
  When running `mempal field-taxonomy --format yaml`
  Then the command fails with an unsupported format error

## Out of Scope

- Do not add a `fields` table or schema migration.
- Do not validate or reject custom field names.
- Do not auto-infer fields during ingest or distill.
- Do not change search/context ranking.
- Do not change wing/room taxonomy behavior.
- Do not add REST field taxonomy API.

## Constraints

- Keep wing/room routing taxonomy separate from mind-model field taxonomy.
- Update `docs/MIND-MODEL-DESIGN.md` so field taxonomy is no longer an open question for Stage 1.
- Keep the surface read-only and side-effect free.
