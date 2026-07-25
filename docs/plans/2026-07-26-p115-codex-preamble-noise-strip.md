# P115 实现计划 — Codex runtime preamble strip + normalize version bump

日期：2026-07-26
Spec：`specs/p115-codex-preamble-noise-strip.spec.md`
背景：PR #7 / #84 / #85 合并后的收尾，含 GitHub issue #10 的修复。

## 步骤

1. **RED（先写失败测试）**
   - `tests/noise_strip.rs`：
     - `<user_instructions>` / `<environment_context>` / `<recommended_plugins>`
       / `<INSTRUCTIONS>` / `<turn_aborted>` 块被整体剥离，块外文本保留；
     - 纯 wrapper 的 user message 在 codex normalize 后不产生 transcript 行；
     - 代码围栏内的 wrapper 标签不剥离。
   - `crates/mempal-runtime/src/ingest/normalize.rs`：钉死
     `CURRENT_NORMALIZE_VERSION == 3`（PR #7 + P115 共同改变了归一化输出，
     `reindex --stale` 依赖版本号发现旧 source）。
   - `crates/mempal-runtime/src/ingest/detect.rs`：`session_meta` +
     `turn_context`-only 文件不判为 Codex；补 `event_msg`/`response_item`
     后判为 Codex。

2. **GREEN（最小实现）**
   - `noise.rs`：把 system-reminder 的成对标签剥离机制泛化为 marker-pair
     列表；`strip_codex_rollout_noise` 使用上述 5 个 wrapper 标签对 +
     既有 session marker 规则。
   - `normalize.rs`：`CURRENT_NORMALIZE_VERSION = 3`。
   - `detect.rs`：`is_codex_jsonl` 要求至少一条 `event_msg` 或
     `response_item`；`turn_context`/`compacted`/未知 type 容忍但不作数。

3. **验证**：workspace `cargo test`、`cargo clippy`、`cargo fmt --check`。

4. **收尾**：更新 `AGENTS.md` / `CLAUDE.md` 的 spec 与 plan inventory；
   commit；`mempal_ingest` 存决策记录；报告 issue triage 结论
   （#1/#2/#3/#4/#9/#11 已修可关闭，#10 由本 P 修复，#81 为 feature 提案
   另立 P）。

## 状态

已完成。
