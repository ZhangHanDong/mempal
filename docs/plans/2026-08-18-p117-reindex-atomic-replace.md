# P117 实现计划 — atomic reindex replace + dry-run feasibility report

日期：2026-08-18
Spec：`specs/p117-reindex-atomic-replace.spec.md`
背景：P115 把 `CURRENT_NORMALIZE_VERSION` 提到 3 后，运维上需要对生产
palace.db 跑一次 `reindex --stale`。评审先后发现六个前置风险：
`ingest_file_with_options` 在 embed 之前就硬删旧 drawers（embed/insert
失败即永久丢数据）；`reindex --stale --dry-run` 在 missing-source 扫描前
提前返回（实测 911 候选 drawers 中约 653 个的 source 文件已不存在，
dry-run 却全部报告为可重处理）；成功但向量数量不足的 embed 响应会被
`zip` 静默截断；`COMMIT` 失败不会 rollback；被 Stage-1/Phase-2 knowledge
引用的 evidence 可能悬空或触发 FK 中止；短哈希插入冲突会被当作 skip
后提交不完整替换。

## 步骤

1. **RED（先写失败测试）**
   - `tests/reindex_safety.rs`：
     - `reindex_embed_failure_preserves_existing_drawers`：FailingEmbedder
       下 reindex 报错且原 drawer 仍在（当前会被删掉）；
     - `reindex_dry_run_reports_missing_sources`：一个 source 文件存在、
       一个不存在，dry-run 报告 skipped_missing 计数（当前恒为 0）；
     - `reindex_replaces_source_without_duplicates`：正常 reindex 无丢失
       无重复（守卫）。
     - `reindex_embedding_count_mismatch_preserves_existing_drawers`：短/长
       向量批次都必须报错且保留旧数据；
     - `reindex_insert_collision_preserves_existing_drawers`：跨 source id
       冲突必须回滚；
     - `reindex_skips_sources_with_knowledge_references`：Stage-1 与 Phase-2
       引用保护 source，dry-run/real-run 都报告并跳过；
     - `replace_transaction_rechecks_knowledge_references`：直接 replace
       绕过外层 report preflight 时，事务内复检仍阻止硬删除；
     - `reindex_cli_reports_governance_protected_sources`：CLI 可见保护计数。
   - store：`with_immediate_transaction_rolls_back_on_error` 与
     `with_immediate_transaction_rolls_back_on_commit_failure`。
2. **GREEN（最小实现）**
   - store：`with_immediate_transaction` helper + source-replace 删除的
     `_in_txn` 变体（公开删除方法保留自带事务）；closure/COMMIT error
     均 best-effort rollback；查询 source 的 Stage-1/Phase-2 引用汇总；
     public replace 统一进入 `_in_txn` 路径并在事务内复检引用。
   - ingest：replace 路径重排为 chunk → dedup（replace 模式跳过
     `drawer_exists`）→ embed → 精确验证向量基数 → 单事务内 delete +
     insert drawers/vectors；replace insert=false 作为冲突错误回滚。
   - reindex：dry-run 与 real-run 都执行 source 文件存在性及 knowledge
     引用 preflight，填充 `skipped_missing_*` / `skipped_protected_*` /
     `protecting_references`；CLI 输出对应计数。
3. **验证**：workspace `cargo test`、`clippy -D warnings`、`fmt --check`、
   `agent-spec parse/lint/lifecycle`（Package/Filter 选择器）。
4. **收尾**：AGENTS.md / CLAUDE.md inventory；PR；`mempal_ingest` 决策。

## 运维顺序（P117 合并后，另行执行）

备份 palace.db → `cargo install --path . --force` → 重启全部 mempal MCP
server（实测 39 个旧进程）与 Claude/Codex 会话 → 验证 normalize v3 与
P116 schema → `reindex --stale --dry-run` 审阅报告 → 执行 reindex。
file-less source（MCP 直录内容）不可重归一化，属预期。

## 状态

已完成。复审扩充后的 10 个 acceptance scenarios 与 boundary check 全部
通过，agent-spec lint 零诊断；workspace test、clippy `-D warnings`、fmt
全部通过。
