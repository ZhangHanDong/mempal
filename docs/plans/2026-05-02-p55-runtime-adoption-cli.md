# P55 Runtime Adoption CLI

## Goal

Expose runtime adoption events through `mempal phase3 adoption record/list/stats`.

## Verification

```bash
agent-spec parse specs/p55-runtime-adoption-cli.spec.md
agent-spec lint specs/p55-runtime-adoption-cli.spec.md --min-score 0.7
cargo test --test phase3_runtime test_cli_phase3_adoption_record_stats_and_gate
```
