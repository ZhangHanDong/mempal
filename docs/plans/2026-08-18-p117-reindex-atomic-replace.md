# P117 实现计划 — atomic reindex replace + dry-run feasibility report

日期：2026-08-18
Spec：`specs/p117-reindex-atomic-replace.spec.md`
背景：P115 把 `CURRENT_NORMALIZE_VERSION` 提到 3 后，运维上需要对生产
palace.db 跑一次 `reindex --stale`。评审发现两个前置风险：
`ingest_file_with_options` 在 embed 之前就硬删旧 drawers（embed/insert
失败即永久丢数据）；`reindex --stale --dry-run` 在 missing-source 扫描前
提前返回（实测 911 候选 drawers 中约 653 个的 source 文件已不存在，
dry-run 却全部报告为可重处理）。

## 步骤

1. **RED（先写失败测试）**
   - `tests/reindex_safety.rs`：
     - `reindex_embed_failure_preserves_existing_drawers`：FailingEmbedder
       下 reindex 报错且原 drawer 仍在（当前会被删掉）；
     - `reindex_dry_run_reports_missing_sources`：一个 source 文件存在、
       一个不存在，dry-run 报告 skipped_missing 计数（当前恒为 0）；
     - `reindex_replaces_source_without_duplicates`：正常 reindex 无丢失
       无重复（守卫）。
   - `crates/mempal-store-sqlite`：`with_immediate_transaction_rolls_back_on_error`。
2. **GREEN（最小实现）**
   - store：`with_immediate_transaction` helper + source-replace 删除的
     `_in_txn` 变体（公开删除方法保留自带事务）。
   - ingest：replace 路径重排为 chunk → dedup（replace 模式跳过
     `drawer_exists`，源感知 id 只可能撞本 source）→ embed → 单事务内
     delete + insert drawers/vectors。
   - reindex：dry-run 也执行 source 文件存在性扫描，填充
     `skipped_missing_*`（CLI 打印已就位）。
3. **验证**：workspace `cargo test`、`clippy -D warnings`、`fmt --check`、
   `agent-spec parse/lint/lifecycle`（Package/Filter 选择器）。
4. **收尾**：AGENTS.md / CLAUDE.md inventory；PR；`mempal_ingest` 决策。

## 运维顺序（P117 合并后，另行执行）

备份 palace.db → `cargo install --path . --force` → 重启全部 mempal MCP
server（实测 39 个旧进程）与 Claude/Codex 会话 → 验证 normalize v3 与
P116 schema → `reindex --stale --dry-run` 审阅报告 → 执行 reindex。
file-less source（MCP 直录内容）不可重归一化，属预期。

## 状态

已完成。
