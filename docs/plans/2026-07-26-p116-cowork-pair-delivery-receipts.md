# P116 实现计划 — cowork pair-push delivery receipts

日期：2026-07-26
Spec：`specs/p116-cowork-pair-delivery-receipts.spec.md`
背景：GitHub issue #81。P8 pair 通道（`mempal_cowork_push` + hook drain）无回执，
partner "没看到消息" 时无法区分未入队 / 未 drain / 已 drain 注入。P88 已给 bus
通道做了 deliveries/ack；本 P 把回执思路移植到 pair 通道，保持零 DB 纯文件设计。

## 步骤

1. **RED（先写失败测试）**
   - `crates/mempal-runtime/src/cowork/receipts.rs`（新模块，含测试）：
     - push 返回确定性 `message_id`（同输入同 id）并追加 `queued` 事件；
     - 带 meta 的 drain 每条消息追加 `drained` 事件（`injected_as` /
       `hook_runtime`）；
     - 状态 join：pending（在 inbox）/ drained（有事件）/ lost（都没有）；
     - 轮转上限 400 事件；
     - 旧格式（无 `message_id`）inbox 行仍可 drain。
   - `tests/cowork_receipts.rs`：`cowork-receipts` CLI plain/json 输出。
   - `crates/mempal-mcp-server`：`CoworkPushResponse.message_id` 在响应与
     output schema required 中。
2. **GREEN（最小实现）**
   - `InboxMessage.message_id: Option<String>`（serde default）；
     `build_message_id`；`push` 返回值带 id 并写 queued 回执（best-effort）；
     `drain_with_receipt` 包装既有 drain 语义并写 drained 回执。
   - `receipts.rs`：`receipts_path` / `append_event` / `load_events` /
     `message_states` / 轮转。
   - CLI：新 `cowork-receipts`；`cowork-drain --hook-runtime`；
     `cowork-status` 加回执 summary 行。
   - MCP：push handler 返回 `message_id`。
3. **验证**：workspace `cargo test`、`cargo clippy`、`cargo fmt --check`。
4. **收尾**：AGENTS.md / CLAUDE.md inventory；commit → PR（refs #81）；
   `mempal_ingest` 决策记录。

## 状态

已完成。
