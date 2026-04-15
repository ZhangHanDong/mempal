spec: task
name: "P8: mempal_cowork_push — bidirectional ephemeral handoff inbox between Claude Code and Codex"
tags: [feature, cowork, mcp, hook, symmetric]
estimate: 1.0d
---

## Intent

给 mempal 新增第 9 个 MCP 工具 `mempal_cowork_push`，让一个 agent（Claude Code 或 Codex）主动向 partner agent 投递一条短消息；对端的 UserPromptSubmit hook 在下次 user turn 时通过 `mempal cowork-drain` CLI 把消息内容作为 context 注入到该 agent 的当前 turn。

这是 mempal cowork 家族的第 3 个 primitive，和前两个**正交**：

- **peek_partner (P6)**：receiver 主动拉 partner 的最近 raw session 流水，receiver 承担过滤负担
- **ingest (P0-P7)**：持久化决策到 `drawers` 表，跨 session / 跨 agent 长期共享
- **cowork_push (P8，本 spec)**：sender 主动投递一条已 curated 的消息，receiver 被动自动接收（at next UserPromptSubmit），ephemeral（drain 即消）

核心用户价值：**消除 "user 作为 relay" 的成本**——agent A 决定某事后可以直接 push 给 agent B，不用人类复述或切窗口提示。

架构是**完全对称**的：Codex 和 Claude Code 都支持 UserPromptSubmit hook 返回 context 注入（Codex via `hookSpecificOutput.additionalContext`，Claude Code via prepend-to-prompt stdout）。一次 spec 覆盖双向。

设计文档：`docs/specs/2026-04-14-p8-cowork-inbox-push.md`（~480 行）。前置能力引用：
- P6 `client_name` capture 在 `src/mcp/server.rs:30-35`，用于 self-push 拒绝和 auto target 推断
- P6 `Tool` enum 和 `infer_partner` 在 `src/cowork/peek.rs`
- Codex 核心原生支持 `additionalContext` 在 `codex-rs/hooks/src/schema.rs:244` 和 `codex-rs/hooks/src/events/user_prompt_submit.rs:290`（已独立验证）

## Decisions

- **新增 MCP 工具**：`mempal_cowork_push`，工具总数 8 → 9
- **MCP 工具签名**：`CoworkPushRequest { content: String, target_tool: Option<String>, cwd: String }` → `CoworkPushResponse { target_tool, inbox_path, pushed_at, inbox_size_after }`
- **存储**：文件 `~/.mempal/cowork-inbox/<target_tool>/<encoded_project_identity>.jsonl`；**不**进 palace.db，**不** bump schema 版本（仍 v4）
- **消息格式**：每行一条 `InboxMessage { pushed_at: String (RFC3339 UTC), from: String, content: String }`，jsonl append
- **Project identity 归一化**：`project_identity(cwd: &Path) -> PathBuf` 爬 ancestor 找 `.git` 目录；命中就返回 git repo root，否则 fallback 到 raw cwd。inbox 文件名由 `encode_project_identity(project_identity(cwd))` 生成，`/` → `-` 替换。保证 repo 内任何子目录的 cwd 解析到同一个 project identity
- **最大单条消息 size**：8 KB per push；超限返回 `InboxError::MessageTooLarge`
- **总 inbox budget (push 端 backpressure)**：`MAX_PENDING_MESSAGES = 16` + `MAX_TOTAL_INBOX_BYTES = 32 KB`。push 前计算 **prospective 状态**（即 "如果这次 append 成功，总量会是多少"）：`existing_count + 1 > MAX_PENDING_MESSAGES` 或 `existing_bytes + content.len() + 1 > MAX_TOTAL_INBOX_BYTES` 任一触发，返回 `InboxError::InboxFull { current_count, current_bytes }`。**检查的是写后总量，不是写前状态**——保证 MAX_TOTAL_INBOX_BYTES 是真正的硬上界，不能被 "existing=32760, push 8KB" 这种 crossing 情况绕过。partner 必须 drain 后才能继续 push
- **drain 原子性 = winner-takes-all at-most-once**：`match fs::rename(path, path.draining) { Ok → 我是 winner，read+delete；Err::NotFound → 空 Vec；Err → fail }`。POSIX rename 原子保证单 winner；loser 立即返回空。**文档必须诚实说明 crash window 会丢 `.draining` 孤儿文件**——不是 exactly-once
- **self-push 拒绝**：caller's client_name 对应的 `Tool` 和 target_tool 相同时返回 `InboxError::SelfPush`，和 P6 self-peek 拒绝同风格
- **target 自动推断**：`target_tool` omit 时，从 caller's `client_name` 反推 partner（`claude-code` → `codex`，`codex` → `claude-code`）
- **path traversal 防守**：`encode_project_identity` 拒绝路径含 `..` 或非 absolute path
- **CLI `mempal cowork-drain`**：`--target <claude|codex>` 必填，`--format <plain|codex-hook-json>` 默认 plain；**cwd 来源必须二选一**：`--cwd <path>`（Claude Code hook 路径，caller 显式传 path）**或** `--cwd-source <source>`（当前唯一支持值 `stdin-json`，从 stdin JSON 解析 `cwd` 字段；Codex hook 路径，基于 `codex-rs/hooks/src/schema.rs:316` 的 `UserPromptSubmitCommandInput.cwd` 官方协议字段）。两者通过 clap `conflicts_with` 互斥。无论哪种来源，CLI 内部都对解析出的 cwd 调 `project_identity` 做 git root 归一化
- **CLI**：新增 `mempal cowork-status --cwd <path>` 子命令（read-only，显示双方 inbox 当前大小和内容预览，用于用户自查 / debug / 第一次装 hook 后验证）
- **CLI**：新增 `mempal cowork-install-hooks [--global-codex]` 子命令，一次装双端 hook 脚本
- **CLI graceful degrade**：`mempal cowork-drain` **永远 exit 0**，任何错误走 stderr，stdout 保持空——保证 hook 失败不阻塞 user prompt
- **Hook stdout format**：
  - `--format plain`（默认）：纯文本 `[Partner inbox from <from> (N messages ...):]\n- <at>: <content>\n...\n[End partner inbox]\n`，供 Claude Code hook 用
  - `--format codex-hook-json`：包 Codex native hook JSON envelope `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"<plain format above>"}}`
- **Hook 安装位置 + cwd 来源协议**：
  - **Claude Code**：项目级 hook 需要两件制品一起落：(1) 脚本文件 `.claude/hooks/user-prompt-submit.sh`（shell script + chmod +x），内容为 `mempal cowork-drain --target claude --cwd "${CLAUDE_PROJECT_CWD:-$PWD}"`；(2) 在 `.claude/settings.json` 的 `hooks.UserPromptSubmit` 数组下注册一条 `{type: "command", command: "bash .claude/hooks/user-prompt-submit.sh"}` 条目。**缺任何一件都等于 hook 不存在**：Claude Code 不按文件名约定自动发现 `.claude/hooks/*.sh`，必须走 `settings.json` 注册路径才会 fire。`install-hooks` 子命令必须同时写/合并这两个文件，合并时保留 `settings.json` 中其它已有 hook（`PostToolUse` 等），遇到 stale drain 条目走自愈替换。Claude Code runtime 不给 hook 脚本传 stdin JSON，所以只能用 env var / shell arg 取 cwd
  - **Codex**：用户级 `~/.codex/hooks.json`，顶层键 `hooks`，嵌套 `UserPromptSubmit`（CamelCase）→ 数组元素 → 嵌套 `hooks` 数组 → `{type: "command", command, statusMessage}` entry。命令 `mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json`。**不写 matcher**——两处独立证据：`codex-rs/hooks/src/events/user_prompt_submit.rs:65-69` 运行时把 matcher_input 硬编码 None，以及 `codex-rs/hooks/src/events/common.rs:98` 里 `matcher_pattern_for_event` 对 UserPromptSubmit 返回 None。按项目 scope 由 `mempal cowork-drain` 从 Codex stdin JSON payload 读 `cwd` 字段（该 payload 由 `codex-rs/hooks/src/schema.rs:316` 的 `UserPromptSubmitCommandInput` 结构体声明，是 Codex 官方协议契约，而非依赖 `current_dir` / `$PWD` 这类 runtime implementation detail）。解析出的 cwd 再通过 `project_identity` 归一化。**运行时依赖 Codex `codex_hooks` feature flag**：shipped `codex-cli ≤ 0.120.0` 该 flag 状态为 "under development" 且默认 `false`，此时 Codex runtime 完全忽略 `~/.codex/hooks.json`。install-hooks 必须读 `~/.codex/config.toml` 检查 `[features].codex_hooks` 或 `features.codex_hooks` 是否为 `true`，否则要在 stdout 明确打印 warning + 告诉用户运行 `codex features enable codex_hooks`。不引入 toml crate；用最小 string-scan parser 只识别 `codex_hooks = true` 字面形式
  - Hook 脚本**禁止**依赖除 `mempal` binary 以外的工具（禁 `jq` / `python3` / `awk`）；stdin JSON parsing 由 Rust 侧的 `mempal cowork-drain --cwd-source stdin-json` 用 serde_json 完成
  - **降级一致**：stdin 读取失败 / JSON parse 失败 / 缺 `cwd` 字段 / 字段类型错误，`mempal cowork-drain` 全部走 graceful degrade（stderr error + exit 0 + 空 stdout），和 Claude Code 侧的错误降级行为对称
- **Protocol Rule 10**：`src/core/protocol.rs` 的 `MEMORY_PROTOCOL` 常量末尾追加 Rule 10 "COWORK PUSH"，包含：何时用 push、at-next-submit 非实时语义、self-push 禁止、"不要用 push 做应该 ingest 的事"、"避免连续 push 造成 overflow" 的 sender 侧行为建议、遇到 `InboxFull` 的退避策略（sender 应停止 push 等 partner drain）
- **Rule 0 工具列表更新**：把 8 个工具列表改成 9 个
- **P6 peek_partner 零修改**：peek.rs 只追加 `Tool::dir_name()` / `Tool::from_name()` / `Tool::partner()` 三个纯函数 helper（如 P6 里还没有），不动任何已有逻辑
- **Install-hooks 输出包含验证步骤**：打印 sample output + "下一步：向 Codex push 一条测试消息，切到 Codex 敲任意 prompt 观察"
- **无新 runtime dependency**：serde / serde_json / tempfile / thiserror 都已有；git repo 检测用纯 `std::fs` 爬 ancestor，不引入 `git2` crate

## Boundaries

### Allowed
- `src/cowork/inbox.rs`（新增）
- `src/cowork/peek.rs`（仅追加 `Tool::dir_name` / `Tool::from_name` / `Tool::partner` 辅助方法；不改任何已有逻辑）
- `src/cowork/mod.rs`（追加 `pub mod inbox` + re-exports）
- `src/mcp/tools.rs`（追加 `CoworkPushRequest` + `CoworkPushResponse` DTO）
- `src/mcp/server.rs`（追加 `mempal_cowork_push` tool handler + `resolve_mempal_home` 辅助，如未有）
- `src/cli.rs`（追加 `cowork-drain` / `cowork-status` / `cowork-install-hooks` 子命令）
- `src/core/protocol.rs`（追加 Rule 10 + Rule 0 工具表 8→9）
- `tests/cowork_inbox.rs`（新增集成测试文件）
- `.claude/hooks/user-prompt-submit.sh`（新增 — install-hooks 子命令会写入这个路径）

### Forbidden
- 不要改 `src/cowork/peek.rs` 中的 `peek_partner` 编排逻辑、cwd 过滤、session 文件扫描、RFC3339 解析、UTC/local-date 窗口——任何已有行为 0 改动
- 不要改 `src/cowork/{claude,codex}.rs` 的 session reader 逻辑
- 不要改 `tests/cowork_peek.rs` 里任何一个现有 P6 测试
- 不要改 `drawers` / `drawer_vectors` / `triples` 任何 DB 表 schema
- 不要 bump `CURRENT_SCHEMA_VERSION`
- 不要改 `SearchResultDto` 或任何 P7 structured signals 代码（`src/aaak/signals.rs` / `src/aaak/codec.rs` pub(crate) 升级 / `src/mcp/tools.rs` SearchResultDto 字段）
- 不要在 `palace.db` 里新建表或列
- 不要让 `mempal_cowork_push` 或 `mempal cowork-drain` 读写 `drawers` / `triples` 表
- 不要让 peek_partner 的 `test_peek_partner_has_no_mempal_side_effects` 回归（该测试必须继续 pass）
- 不要引入任何新的 runtime 或 dev dependency
- 不要改 `mempal_search` / `mempal_ingest` / `mempal_peek_partner` 等已有 8 个 MCP 工具的 schema 或行为
- 不要修改 Codex 上游源码（`/Users/zhangalex/Work/Projects/AI/codex`）
- 不要让 hook 脚本依赖除 `mempal` binary 以外的任何外部工具（`jq` / `python3` / `awk` 等都不许）

## Out of Scope

- 真正意义的实时 mid-turn 注入（需要 Codex / Claude Code runtime watch session 文件，属于上游 feature）
- 消息加密 / 访问控制 / 鉴权（单用户单机，filesystem permission 足够）
- 跨机器 cowork（需要新传输层）
- 消息 TTL / retention / 历史（drained = gone）
- Priority / urgency 字段
- Broadcast / fanout（一次 push 一个 target）
- Reply / threading / message ID（fire-and-forget）
- Rich content（tool_use / file attachment / image，只支持 plain text body）
- 自动 push 触发器（Codex 自己判断何时 push）——留给 agent 在 prompt 上下文里自己决定
- `mempal cowork-mute --target <tool>`（焦点模式静默，留到 future spec）
- Codex 项目级 hook（`<project>/.codex/hooks.json`）——Codex 上游只支持 `~/.codex/hooks.json`
- 给 `mempal_peek_partner` / `mempal_ingest` / `mempal_kg` / `mempal_search` 等任何其他 MCP 工具加 cowork 能力
- 修改 `mempal compress` / `wake-up --format aaak` 等 CLI 子命令
- longmemeval benchmark 评估 push 效率

## Completion Criteria

Scenario: push 到 partner inbox 后 drain 返回内容字节级相等
  Test:
    Filter: test_push_drain_round_trip_preserves_content
    Level: integration
    Test Double: tempdir_mempal_home
    Targets: src/cowork/inbox.rs, src/mcp/server.rs, src/cli.rs
  Given tempdir 作为 mempal_home，cwd = "/tmp/fake-project-1"
  And 通过 `inbox::push` 写入 content = "hello from claude, P8 test #1"，caller = Claude，target = Codex
  When 调用 `inbox::drain(target=Codex, cwd)` 读出 messages
  Then 返回的 Vec 长度恰好为 "1"
  And 第 "0" 条 message.content 字节级等于原始 "hello from claude, P8 test #1"
  And message.from == "claude"

Scenario: Unicode content（CJK + emoji + 引号）push-drain round trip 字节级保全
  Test:
    Filter: test_unicode_content_round_trip_preserves_bytes
    Level: integration
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home
  And content = "决策：采用 Arc<Mutex<>> 🔒 因为 'shared ownership' 需要"
  When push + drain
  Then drain 出来的 content 字符串和原始 content 字节级相等（byte-level assert_eq）
  And content 中的 `"` / `\n` / `<` / `>` 字符未被 escape

Scenario: drain 空 inbox 返回空 Vec 不报错
  Test:
    Filter: test_drain_empty_inbox_returns_empty_vec
    Level: unit
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home，inbox 文件不存在
  When 调用 `inbox::drain(target=Claude, cwd)`
  Then 返回 `Ok(vec![])`
  And 没有创建任何新文件

Scenario: drain 不存在的 inbox 目录返回空 Vec
  Test:
    Filter: test_drain_nonexistent_inbox_dir
    Level: unit
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home 不含 cowork-inbox 目录
  When 调用 `inbox::drain(target=Codex, cwd)`
  Then 返回 `Ok(vec![])`
  And 没有报 `std::io::Error`

Scenario: push 超过 8 KB 返回 MessageTooLarge
  Test:
    Filter: test_push_over_8kb_rejected
    Level: unit
    Targets: src/cowork/inbox.rs
  Given content 长度 = "8193" 字节
  When 调用 `inbox::push(..., content, ...)`
  Then 返回 `Err(InboxError::MessageTooLarge(8193))`
  And inbox 文件未被创建或未被 append

Scenario: push 到包含 ".." 的 cwd 被 InvalidCwd 拒绝
  Test:
    Filter: test_push_invalid_cwd_with_parent_traversal
    Level: unit
    Targets: src/cowork/inbox.rs
  Given cwd = "/Users/zhangalex/../etc"
  When 调用 `inbox::push(..., cwd, ...)`
  Then 返回 `Err(InboxError::InvalidCwd(_))`
  And 没有任何文件被写入 mempal_home

Scenario: self-push（caller 和 target 相同）被拒绝
  Test:
    Filter: test_self_push_rejected_symmetric_to_peek_self_rejection
    Level: unit
    Targets: src/cowork/inbox.rs
  Given caller = Tool::Codex，target = Tool::Codex
  When 调用 `inbox::push(caller, target, ...)`
  Then 返回 `Err(InboxError::SelfPush(Tool::Codex))`
  And inbox 文件未被创建

Scenario: 连续 3 次 push 后 drain 一次，返回顺序和 push 顺序一致
  Test:
    Filter: test_multiple_push_preserves_fifo_order_on_drain
    Level: integration
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home
  And push "message-alpha"，然后 push "message-beta"，然后 push "message-gamma"
  When drain 一次
  Then 返回 Vec 长度为 "3"
  And vec[0].content == "message-alpha"
  And vec[1].content == "message-beta"
  And vec[2].content == "message-gamma"

Scenario: drain 之后 inbox 文件消失，第二次 drain 同一 (target, cwd) 返回空
  Test:
    Filter: test_drain_is_one_shot_file_disappears_after_drain
    Level: integration
    Targets: src/cowork/inbox.rs
  Given 先 push 1 条消息到 (target=Claude, cwd)
  And drain 一次，拿到 1 条消息
  When 再次调用 drain(target=Claude, cwd)
  Then 返回空 Vec
  And inbox 文件不存在

Scenario: 不同 project 的 cwd 隔离
  Test:
    Filter: test_drain_is_isolated_per_distinct_project
    Level: integration
    Targets: src/cowork/inbox.rs
  Given tempdir 作为 mempal_home
  And 两个独立 git repo：`<tmp>/project-alpha` 和 `<tmp>/project-beta`，每个都有 `.git/` 目录
  And 调用 `push(target=Codex, cwd="<tmp>/project-alpha", content="alpha msg")`
  When 调用 `drain(target=Codex, cwd="<tmp>/project-beta")`
  Then 返回空 Vec
  And project-alpha 的 inbox 文件仍然存在，未被 drain

Scenario: 同一 git repo 的 repo root 和子目录 cwd 解析到同一个 inbox（project identity 归一化）
  Test:
    Filter: test_project_identity_normalizes_subdir_to_git_root
    Level: integration
    Targets: src/cowork/inbox.rs
  Given tempdir 作为 mempal_home
  And 一个 git repo `<tmp>/project-gamma` 含 `.git/` 目录
  And 子目录 `<tmp>/project-gamma/src/cowork` 已创建
  And 调用 `push(target=Claude, cwd="<tmp>/project-gamma", content="from repo root")`（从 repo root push）
  When 调用 `drain(target=Claude, cwd="<tmp>/project-gamma/src/cowork")`（从子目录 drain）
  Then 返回 Vec 长度 == 1
  And 第 0 条 message.content == "from repo root"
  And `project_identity("<tmp>/project-gamma/src/cowork")` 等于 `project_identity("<tmp>/project-gamma")`（都归一到 repo root）

Scenario: format_codex_hook_json 产出合法 JSON 且 additionalContext 等于 plain format 输出
  Test:
    Filter: test_codex_hook_json_envelope_wraps_plain_format_correctly
    Level: unit
    Targets: src/cowork/inbox.rs
  Given 一个 2-message Vec 的 InboxMessage
  When 同时调用 `format_plain(from, &messages)` 和 `format_codex_hook_json(from, &messages)`
  Then codex-hook-json 输出是合法的 JSON（可被 `serde_json::from_str` 解析）
  And 解析后的 JSON 有字段 `hookSpecificOutput.hookEventName == "UserPromptSubmit"`
  And 解析后的 JSON 中 `hookSpecificOutput.additionalContext` 字符串等于 format_plain 的输出
  And content 中的引号 / 换行 / 反斜杠在 JSON 里被正确 escape

Scenario: push 和 drain 不改变 palace.db 的任何 invariant
  Test:
    Filter: test_push_and_drain_have_no_palace_db_side_effects
    Level: integration
    Test Double: tempfile_palace_db
    Targets: src/cowork/inbox.rs, src/mcp/server.rs
  Given tempfile palace.db 基线：drawer_count = "N"，triple_count = "M"，schema_version = 4
  When 执行 "3" 次 push 和 "2" 次 drain
  Then drawer_count 仍为 "N"
  And triple_count 仍为 "M"
  And schema_version 仍为 4

Scenario: 并发两个 drain 调用 winner-takes-all（at-most-once）
  Test:
    Filter: test_concurrent_drain_is_winner_takes_all_at_most_once
    Level: integration
    Targets: src/cowork/inbox.rs
  Given push "3" 条消息到 (target=Codex, cwd)
  When 同时 spawn 两个 tokio task 各自调用 drain(target=Codex, cwd)
  Then 恰好一个 task 返回 Vec 长度 == "3"（winner）
  And 另一个 task 返回 Vec 长度 == "0"（loser）
  And 任意 message content 不在两个 Vec 里同时出现（无 duplicate delivery）
  And 两次 drain 完成后，inbox 文件不存在

Scenario: mempal cowork-drain CLI 在 mempal home 不存在时仍 exit 0 + 空 stdout
  Test:
    Filter: test_cowork_drain_cli_graceful_degrade_when_mempal_home_missing
    Level: integration
    Targets: src/cli.rs, src/cowork/inbox.rs
  Given tempdir 作为 fake HOME，不预创建 `.mempal/` 目录
  When 执行 `mempal cowork-drain --target claude --cwd /tmp/fake` 并捕获 stdout + exit code
  Then exit code == 0
  And stdout 为空字符串
  And 进程未 panic（未产生 SIGABRT）

Scenario: mempal cowork-status CLI 列出双方 inbox 当前状态
  Test:
    Filter: test_cowork_status_cli_lists_both_inboxes
    Level: integration
    Targets: src/cli.rs, src/cowork/inbox.rs
  Given push "2" 条消息到 target=Claude + cwd=X
  And push "1" 条消息到 target=Codex + cwd=X
  When 执行 `mempal cowork-status --cwd X`
  Then stdout 包含字符串 "claude inbox"
  And stdout 包含字符串 "2 messages"
  And stdout 包含字符串 "codex inbox"
  And stdout 包含字符串 "1 message"
  And cowork-status 命令本身**不** drain（调用后 push 计数仍为 2 + 1）

Scenario: mempal_cowork_push MCP 工具调用在 client_info 未提供时拒绝自动推断
  Test:
    Filter: test_mcp_push_without_client_info_rejects_auto_target
    Level: integration
    Targets: src/mcp/server.rs
  Given MempalMcpServer 未收到 `initialize` 回调（client_name 为 None）
  When 调用 mempal_cowork_push 且 target_tool omit
  Then 返回 MCP InvalidParams error
  And 没有任何文件被写入 cowork-inbox 目录

Scenario: mempal cowork-install-hooks 子命令写入 Claude Code hook 脚本到项目级路径
  Test:
    Filter: test_cowork_install_hooks_writes_claude_hook_script_with_exec_bit
    Level: integration
    Targets: src/cli.rs
  Given tempdir 作为 fake 项目 cwd
  And 该 cwd 内不存在 `.claude/hooks/user-prompt-submit.sh` 文件
  When 执行 `mempal cowork-install-hooks`（在该 cwd 内，不带 --global-codex）
  Then 文件 `.claude/hooks/user-prompt-submit.sh` 在该 cwd 下被创建
  And 文件内容包含字符串 "mempal cowork-drain"
  And 文件内容包含字符串 "--target claude"
  And 文件 mode 包含 executable bit（owner execute 位被设置）
  And stdout 打印了验证步骤 guidance

Scenario: P6 peek_partner 整套既有集成测试在 P8 改动后继续 pass（零回归不变量）
  Test:
    Filter: test_p6_peek_partner_suite_remains_green_after_p8_changes
    Level: integration
    Targets: src/cowork/peek.rs, src/cowork/claude.rs, src/cowork/codex.rs, tests/cowork_peek.rs
  Given P8 改动已合入（新增 `src/cowork/inbox.rs` + 其他 Allowed list 里的改动）
  When 执行 `cargo test --no-default-features --features model2vec --test cowork_peek`
  Then exit code == 0
  And 所有 "7" 个 P6 既有测试报告 passed
  And 没有任何 P6 测试被 ignored / filtered / modified

Scenario: 第 17 次 push 被 InboxFull 拒绝（prospective count 限制）
  Test:
    Filter: test_push_rejected_when_prospective_count_would_exceed_limit
    Level: unit
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home
  And 连续 push "16" 条消息到 (target=Claude, cwd)，每条 content == "a"（远低于字节上限）
  When 调用 push 第 "17" 条消息
  Then 返回 `Err(InboxError::InboxFull { current_count: 16, current_bytes: _ })`
  And inbox 文件的 line 数仍为 "16"（未 append 第 17 条）

Scenario: push 跨越 32 KB 上限被 InboxFull 拒绝（prospective byte crossing threshold）
  Test:
    Filter: test_push_rejected_when_prospective_bytes_would_cross_limit
    Level: unit
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home
  And 当前 inbox 文件字节数 == "32700"，pending message 条数 == "10"（两项都远低于 count 上限 16）
  When 调用 push 一条 `content.len() == "200"` 的消息（prospective_bytes = 32700 + 200 + 1 = 32901 > 32768）
  Then 返回 `Err(InboxError::InboxFull { current_count: 10, current_bytes: 32700 })`
  And inbox 文件字节数仍为 "32700"（未 append）
  And inbox 文件 line 数仍为 "10"

Scenario: push 刚好达到 32 KB 上限但未越界被接受（prospective byte exact boundary accepted）
  Test:
    Filter: test_push_accepted_at_exact_byte_limit_boundary
    Level: unit
    Targets: src/cowork/inbox.rs
  Given tempdir mempal_home
  And 构造 inbox 初始字节数 `existing_bytes` 和一条新消息 `content`，使得 `existing_bytes + content.len() + 1 == 32768`（即 prospective_bytes 恰好等于 MAX_TOTAL_INBOX_BYTES，没有严格大于）
  And pending message 条数 < "16"（避免触发 count 限制）
  When 调用 push 该条消息
  Then 返回 `Ok((path, 32768))`
  And inbox 文件字节数变为 "32768"
  And inbox 文件 line 数增加 "1"
  And 此断言验证 `>` vs `>=` 的边界精度：prospective_bytes 恰好等于 limit 时必须接受，严格大于 limit 时才拒绝

Scenario: mempal cowork-drain 通过 --cwd-source stdin-json 从 stdin JSON 读取 cwd 并 drain（Codex hook 路径）
  Test:
    Filter: test_cowork_drain_reads_cwd_from_stdin_json_codex_path
    Level: integration
    Targets: src/cli.rs, src/cowork/inbox.rs
  Given tempdir mempal_home
  And 一个 git repo 作为目标 cwd，路径 `<tmp>/project-delta`
  And push "1" 条消息到 (target=Codex, cwd="<tmp>/project-delta") content 为 "stdin json test"
  And 构造 Codex UserPromptSubmit hook stdin payload（完整 JSON）：`{"session_id":"s1","turn_id":"t1","transcript_path":null,"cwd":"<tmp>/project-delta","hook_event_name":"UserPromptSubmit","model":"gpt-5-codex","permission_mode":"workspace-write","prompt":"继续"}`
  When 执行 `mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json`，把上述 JSON 通过 stdin 管道传入
  Then exit code == 0
  And stdout 包含字符串 "stdin json test"
  And stdout 是合法的 Codex hook JSON 包络（可 `serde_json::from_str` 解析，含 `hookSpecificOutput.hookEventName == "UserPromptSubmit"` 和 `hookSpecificOutput.additionalContext` 字段）
  And 调用后 (target=Codex, cwd="<tmp>/project-delta") 的 inbox 文件不存在（已 drain）

Scenario: mempal cowork-drain --cwd-source stdin-json 对 malformed stdin 走 graceful degrade
  Test:
    Filter: test_cowork_drain_stdin_json_malformed_payload_graceful_degrade
    Level: integration
    Targets: src/cli.rs
  Given 三个独立子用例，每个执行 `mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json` 并从 stdin 传入不同的坏载荷：
    - 子用例 A：stdin 是非 JSON 纯文本 `"not json at all"`
    - 子用例 B：stdin 是合法 JSON 但缺 `cwd` 字段，例如 `{"session_id":"s","turn_id":"t","prompt":"继续"}`
    - 子用例 C：stdin 是合法 JSON 且有 `cwd` 字段但类型错误，例如 `{"cwd":42}`
  When 三个子用例分别执行 `mempal cowork-drain --target codex --format codex-hook-json --cwd-source stdin-json` 并等待进程退出
  Then 三个子用例的 exit code 都 == 0
  And 三个子用例的 stdout 都是空字符串
  And 三个子用例的进程都没有 panic（未产生 SIGABRT）
  And `~/.mempal/cowork-inbox/codex/` 目录下的任何文件都未被读、改、删除（验证方法：调用前后 sha256 + mtime 完全相等）

Scenario: mempal cowork-install-hooks --global-codex 写入的 ~/.codex/hooks.json 严格匹配 Codex 嵌套 schema
  Test:
    Filter: test_cowork_install_hooks_writes_correct_codex_hooks_json_shape
    Level: integration
    Targets: src/cli.rs, src/cowork/inbox.rs
  Given tempdir 作为 fake HOME（`$HOME` 被 override）
  And 该 HOME 内不存在 `.codex/hooks.json` 文件
  And 当前 cwd 是一个 tempdir git repo `<tmp>/project-epsilon`
  When 执行 `mempal cowork-install-hooks --global-codex`
  Then 文件 `$HOME/.codex/hooks.json` 存在
  And 该文件是合法 JSON（可 `serde_json::from_str::<Value>` 解析）
  And parsed JSON 有顶层字段 `hooks`（对象类型）
  And `hooks.UserPromptSubmit` 是数组类型且长度 >= "1"
  And `hooks.UserPromptSubmit[0].hooks` 是数组类型且长度 >= "1"
  And `hooks.UserPromptSubmit[0].hooks[0].type` == `"command"`
  And `hooks.UserPromptSubmit[0].hooks[0].command` 是 string 且包含子串 `"mempal cowork-drain"`
  And `hooks.UserPromptSubmit[0].hooks[0].command` 包含子串 `"--target codex"`
  And `hooks.UserPromptSubmit[0].hooks[0].command` 包含子串 `"--format codex-hook-json"`
  And `hooks.UserPromptSubmit[0].hooks[0].command` 包含子串 `"--cwd-source stdin-json"`
  And `hooks.UserPromptSubmit[0].hooks[0].command` **不**包含子串 `"$PWD"`（validate round-2 $PWD removal）
  And `hooks.UserPromptSubmit[0].hooks[0]` **不**含字段 `matcher`（UserPromptSubmit 的 matcher 会被 Codex runtime 忽略，写入也没意义）
