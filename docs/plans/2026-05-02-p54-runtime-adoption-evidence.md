# P54 Runtime Adoption Evidence

## Goal

Add schema v9 `runtime_adoption_events` and DB APIs for Phase-3 runtime
adoption evidence.

## Verification

```bash
agent-spec parse specs/p54-runtime-adoption-evidence.spec.md
agent-spec lint specs/p54-runtime-adoption-evidence.spec.md --min-score 0.7
cargo test --test phase3_runtime test_runtime_adoption_event_roundtrip_db
```
