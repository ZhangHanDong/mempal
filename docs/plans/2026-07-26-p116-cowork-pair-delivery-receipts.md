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
     drain 与 `record_drained` 保持两阶段 API，调用者仅在注入成功后写
     drained 回执。
   - receipt 锁不可用或为 Windows no-op 时，用进程号 + 纳秒时间戳 +
     原子序号降级后缀保持成功 push 的 handle 唯一性；used-id 来源读取或
     解析不完整时也走同一路径，不信任残缺快照。
   - `receipts.rs`：`receipts_path` / `append_event` / `load_events` /
     `message_states` / 轮转。
   - 状态查询只把 `NotFound` 当成 inbox 缺失；其他读取失败显式返回，
     避免把不可读 live inbox 误报为 `lost`；同秒事件用稳定全序打破并列。
   - CLI：新 `cowork-receipts`；`cowork-drain --hook-runtime`；
     `cowork-status` 按 target 各加一行回执 summary。
   - MCP：push handler 返回 `message_id`。
3. **验证**：workspace `cargo test`、`cargo clippy`、`cargo fmt --check`。
4. **收尾**：AGENTS.md / CLAUDE.md inventory；commit → PR（refs #81）；
   `mempal_ingest` 决策记录。

## 状态

已完成。
