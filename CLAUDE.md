# mempal

Rust 实现的 coding agent 项目记忆工具。单二进制，`cargo install mempal`，10 秒内带出处找回历史决策。

## Skills

**必须使用项目内的 Rust 技能**：`skills/rust-skills/SKILL.md`

编写、审查、调试、重构 Rust 代码时，遵循该 skill 的四步工作流（理解 → 服从 → 释放 → 约束）和概念锚点框架。

## 参考实现

mempal 借鉴 MemPalace 的设计理念（verbatim 存储、Wing/Room 结构、AAAK 压缩），用 Rust 从零实现并修复其缺陷。以下两个本地项目是关键参考：

- **MemPalace 源码**：`/Users/zhangalex/Work/Projects/AI/mempalace` — Python 原版实现，查看 `mempalace/` 目录下的 searcher.py、palace_graph.py、dialect.py、knowledge_graph.py 等模块了解原始设计
- **MemPalace 书稿**：`/Users/zhangalex/Work/Projects/AI/mempalace-book` — 基于源码的设计分析书，`book/src/` 下 30 章（含 Part 10 mempal Rust 重铸）+ 4 个附录

实现时遇到设计疑问，优先查阅书稿中的分析（特别是附录 C 的 AAAK 评估和附录 A/B 的 E2E Trace），而非直接复制 Python 代码。

## 设计文档

`docs/specs/2026-04-08-mempal-design.md` — 完整架构设计，所有实现必须以此为准。

## Spec 体系

项目使用 agent-spec 管理任务合约。所有实现必须对照 spec 验收。

硬规则：

- 每一个 numbered P 都必须留下 `specs/pNN-*.spec.md`。
- 每一个 numbered P 都必须留下匹配的 `docs/plans/*pNN*.md`。
- 该规则同样适用于文档-only、audit-only、policy-only 和代码实现类 P。
- future spec-less P 不能算完成；missing spec 必须先补齐再实现或合并。
- 完成后必须同步更新 `AGENTS.md` / `CLAUDE.md` 的 spec 与 plan inventory。

### 项目级 Spec
- `specs/project.spec.md` — 项目约束（edition、依赖、编码规范、架构不变量）

### 已完成的 Spec（P0-P28）

| Spec | 状态 | 范围 |
|------|------|------|
| `specs/p0-core-scaffold.spec.md` | 完成 | workspace 骨架 + SQLite schema |
| `specs/p0-embed-trait.spec.md` | 完成 | Embedder trait（model2vec 默认 + ort 可选） |
| `specs/p0-ingest.spec.md` | 完成 | 导入管道（格式检测/归一化/分块/存储） |
| `specs/p0-search-cli.spec.md` | 完成 | 搜索引擎 + CLI |
| `specs/p1-routing-citation.spec.md` | 完成 | 查询路由 + 引用组装 |
| `specs/p2-mcp.spec.md` | 完成 | MCP 服务器（7 工具） |
| `specs/p3-aaak.spec.md` | 完成 | AAAK 编解码（BNF + 往返验证） |
| `specs/p4-rest-api.spec.md` | 完成 | REST API（feature-gated） |
| `specs/p5-wake-up-importance.spec.md` | 完成 | L1 重要性排序 wake-up（schema v4） |
| `specs/p5-kg-timeline-stats.spec.md` | 完成 | KG timeline + stats actions |
| `specs/p5-semantic-dedup.spec.md` | 完成 | 语义去重检测（ingest warning） |
| `specs/p5-agent-diary.spec.md` | 完成 | Agent 日记 convention（协议层） |
| `specs/p5-format-support.spec.md` | 完成 | Slack DM + Codex CLI 格式支持 |
| `specs/p6-cowork-peek-and-decide.spec.md` | 完成 | Claude↔Codex 协作：live session peek（`mempal_peek_partner`）+ Rule 8/9 |
| `specs/p7-search-structured-signals.spec.md` | 完成 | `mempal_search` 响应每条结果附带 5 个 AAAK-derived 结构化字段（`entities` / `topics` / `flags` / `emotions` / `importance_stars`），`content` 保持 raw |
| `specs/p8-cowork-inbox-push.spec.md` | 完成 | 双向 cowork push — `mempal_cowork_push` MCP 工具 + `cowork-drain` / `cowork-status` / `cowork-install-hooks` CLI + 对称 UserPromptSubmit hook 注入（at-next-submit 交付） |
| `specs/p9-fact-checker.spec.md` | 完成 | 离线事实核查 — `mempal_fact_check` MCP 工具 + `fact-check` CLI，基于 KG triples + 已知 entity 检测 SimilarNameConflict / RelationContradiction / StaleFact（协议 Rule 11） |
| `specs/p9-ingest-lock.spec.md` | 完成 | Per-source `flock` 锁 — 消除 Claude↔Codex 并发 ingest 同一 source 的 TOCTOU race；`IngestStats` / `IngestResponse.lock_wait_ms` 提供并发等待可观测性 |
| `specs/p10-explicit-tunnels.spec.md` | 完成 | schema v6 + `mempal_tunnels` 扩 add/list/delete/follow 显式跨 wing 链接 |
| `specs/p10-normalize-version.spec.md` | 完成 | schema v7 `normalize_version` 列 + `reindex --stale` 机制 |
| `specs/p11-transcript-noise-strip.spec.md` | 完成 | Claude/Codex transcript noise strip + `CURRENT_NORMALIZE_VERSION=2` |
| `specs/p11-chunk-neighbors.spec.md` | 完成 | `mempal_search` / CLI search 可选返回命中 chunk 前后邻居 |
| `specs/p11-diary-daily-rollup.spec.md` | 完成 | `agent-diary` 天粒度 upsert drawer，防 chatty agent 爆炸 |
| `specs/p12-mind-model-bootstrap.spec.md` | 完成 | Stage-1 mind-model bootstrap：typed drawers + `dao/shu/qi` 最小治理字段 + `global/repo/worktree` anchor metadata |
| `specs/p13-wake-up-statement.spec.md` | 完成 | wake-up 最小闭环：knowledge drawer 优先按 `statement` 唤醒，evidence 继续按 `content` 唤醒 |
| `specs/p13-ingest-identity.spec.md` | 完成 | typed/bootstrap ingest `drawer_id` identity parity：MCP / REST / 文件入口统一使用 bootstrap identity components |
| `specs/p14-context-assembler.spec.md` | 完成 | mind-model runtime assembler：`mempal context` 按 `dao_tian -> dao_ren -> shu -> qi -> evidence` 和 `worktree -> repo -> global` 组装 context pack |
| `specs/p15-mcp-context.spec.md` | 完成 | `mempal_context` MCP 工具：向 agent runtime 暴露 P14 mind-model context pack |
| `specs/p16-context-skill-guidance.spec.md` | 完成 | context-guided skill selection protocol：`mempal_context` 辅助 workflow/skill/tool 选择，但 `trigger_hints` 只做 bias、不自动执行 |
| `specs/p17-knowledge-lifecycle.spec.md` | 完成 | bootstrap knowledge lifecycle CLI：`mempal knowledge promote/demote` 受约束更新 knowledge drawer status 与 refs，并写 audit |
| `specs/p18-knowledge-distill.spec.md` | 完成 | bootstrap knowledge distill CLI：`mempal knowledge distill` 从 evidence refs 创建 candidate knowledge drawer |
| `specs/p19-lifecycle-ref-validation.spec.md` | 完成 | lifecycle evidence ref hardening：`promote/demote` refs 必须是存在的 evidence drawers |
| `specs/p20-promotion-gate-policy.spec.md` | 完成 | read-only promotion gate policy：`mempal knowledge gate` 评估 knowledge drawer 是否满足最小提升门槛 |
| `specs/p21-mcp-knowledge-gate.spec.md` | 完成 | `mempal_knowledge_gate` MCP 工具：向 agent runtime 暴露 P20 read-only promotion gate |
| `specs/p22-mcp-knowledge-distill.spec.md` | 完成 | `mempal_knowledge_distill` MCP 工具：从 evidence refs 创建 candidate knowledge drawer |
| `specs/p23-mcp-knowledge-lifecycle.spec.md` | 完成 | `mempal_knowledge_promote` / `mempal_knowledge_demote` MCP 工具：gate-enforced promotion + evidence-backed demotion |
| `specs/p24-anchor-publication.spec.md` | 完成 | `mempal knowledge publish-anchor` CLI：显式 outward anchor publication（worktree -> repo -> global） |
| `specs/p25-mcp-anchor-publication.spec.md` | 完成 | `mempal_knowledge_publish_anchor` MCP 工具：显式 outward anchor publication |
| `specs/p26-dao-tian-runtime-budget.spec.md` | 完成 | `mempal context` / `mempal_context` 默认最多注入 1 条 `dao_tian`，支持显式禁用或提高预算 |
| `specs/p27-knowledge-policy-surface.spec.md` | 完成 | `mempal knowledge policy` / `mempal_knowledge_policy`：只读 Stage-1 promotion policy 阈值表 |
| `specs/p28-field-taxonomy-surface.spec.md` | 完成 | `mempal field-taxonomy` / `mempal_field_taxonomy`：只读 Stage-1 field taxonomy guidance |
| `specs/p29-wake-up-context-boundary.spec.md` | 完成 | 固化 wake-up 与 mind-model context 边界：wake-up 保持 L0/L1 refresh，typed `dao/shu/qi` 组装只属于 `mempal context` / `mempal_context` |
| `specs/p30-knowledge-card-storage-boundary.spec.md` | 完成 | 固化 Phase-2 knowledge card 存储边界：未来 `knowledge_cards` 使用同一个 SQLite `palace.db` 的独立表，不拆外部 persistence layer |
| `specs/p31-knowledge-card-schema.spec.md` | 完成 | schema v8 Phase-2 `knowledge_cards` / `knowledge_evidence_links` / `knowledge_events` 最小 schema contract |
| `specs/p32-knowledge-card-schema-v8.spec.md` | 完成 | schema v8 migration：新增 Phase-2 knowledge card 三表、约束、索引、append-only events |
| `specs/p33-knowledge-card-core-api.spec.md` | 完成 | Phase-2 knowledge card DB core API：Rust types + card/link/event create/read/update/list，不暴露 CLI/MCP/REST |
| `specs/p34-knowledge-card-cli.spec.md` | 完成 | Phase-2 knowledge card 最小 CLI 管理入口：create/get/list/link/event/events，不接入 MCP/REST/search/context |
| `specs/p35-knowledge-card-mcp-read.spec.md` | 完成 | Phase-2 knowledge card MCP 只读入口：`mempal_knowledge_cards` list/get/events，不开放写操作 |
| `specs/p36-knowledge-card-backfill-report.spec.md` | 完成 | Stage-1 knowledge drawer -> Phase-2 card 只读 backfill-plan report；dry-run，不迁移 |
| `specs/p37-knowledge-card-backfill-apply.spec.md` | 完成 | Stage-1 knowledge drawer -> Phase-2 card 显式 backfill apply：默认 dry-run，`--execute` 创建 cards/links/events |
| `specs/p38-knowledge-card-gate.spec.md` | 完成 | Phase-2 knowledge card gate：按 role-separated evidence links 评估提升门槛 |
| `specs/p39-knowledge-card-lifecycle-cli.spec.md` | 完成 | Phase-2 knowledge card CLI lifecycle：gate-enforced promote + evidence-backed demote |
| `specs/p40-mcp-knowledge-card-lifecycle.spec.md` | 完成 | `mempal_knowledge_cards` 扩展 gate/promote/demote actions |
| `specs/p41-knowledge-card-runtime-boundary.spec.md` | 完成 | 固化 Phase-2 card runtime boundary：cards 已治理，但尚非默认 context/search source |
| `specs/p42-mind-model-completion-audit.spec.md` | 完成 | MIND-MODEL P42 baseline completion audit + future work 明确化 |
| `specs/p43-knowledge-card-retrieval-contract.spec.md` | 完成 | Phase-2 card retrieval contract：定义 card result + evidence citation 形状，不改默认 runtime 行为 |
| `specs/p44-card-context-assembler.spec.md` | 完成 | `mempal context` / `mempal_context` 显式 `include_cards`：按 P43 contract 注入 active cards + evidence citations |
| `specs/p45-card-linked-evidence-retrieval.spec.md` | 完成 | `mempal knowledge-card retrieve` / `mempal_knowledge_cards action=retrieve`：通过 linked evidence 检索 active cards，不改默认 search |
| `specs/p46-card-context-default-policy.spec.md` | 完成 | P46 card context default policy：card-aware context 继续 opt-in；未来默认启用必须有 runtime evidence 和 rollback criteria |
| `specs/p47-card-embedding-policy.spec.md` | 完成 | P47 card embedding policy：暂不加 card-level embeddings；未来实现必须证明 statement-match misses、处理 stale vectors 和 rollback |
| `specs/p48-card-audit-policy.spec.md` | 完成 | P48 card audit policy：`knowledge_events` 是 Phase-2 card lifecycle 权威审计；不默认双写 JSONL |
| `specs/p49-research-ingestion-policy.spec.md` | 完成 | P49 research ingestion policy：research-rs 输出只进 evidence / evidence-backed candidate insights，不可直接定义 dao |
| `specs/p50-evaluator-promotion-policy.spec.md` | 完成 | P50 evaluator promotion policy：evaluator 只能 advisory，不可绕过 deterministic gates / human review |
| `specs/p51-mind-model-closure-audit.spec.md` | 完成 | P51 mind model closure audit：确认 P12-P50 baseline 已完成，未来扩展必须开新阶段 spec |
| `specs/p52-phase-3-intake-roadmap.spec.md` | 完成 | P52 phase 3 intake roadmap：定义 baseline 后新阶段候选轨道与 evidence/rollback/acceptance 入口规则 |
| `specs/p53-phase-3-candidate-evidence-audit.spec.md` | 完成 | P53 phase 3 candidate evidence audit：评估 Phase-3 候选证据，推荐先做 runtime adoption evidence |
| `specs/p54-runtime-adoption-evidence.spec.md` | 完成 | P54 runtime adoption evidence：schema v9 `runtime_adoption_events` + DB API |
| `specs/p55-runtime-adoption-cli.spec.md` | 完成 | P55 runtime adoption CLI：`mempal phase3 adoption record/list/stats` |
| `specs/p56-card-context-default-gate.spec.md` | 完成 | P56 card context default gate：read-only Phase-3 gate，不改 `include_cards` 默认值 |
| `specs/p57-card-embedding-evidence-gate.spec.md` | 完成 | P57 card embedding evidence gate：card embeddings 需 measured miss evidence，未新增 vector schema |
| `specs/p58-evaluator-api-evidence-gate.spec.md` | 完成 | P58 evaluator API evidence gate：evaluator API 仍 advisory-only，不写 lifecycle |
| `specs/p59-research-adapter-ingestion-contract.spec.md` | 完成 | P59 research adapter ingestion contract：`phase3 research-validate-plan` 验证外部 report contract，不自动 ingest |
| `specs/p60-mcp-phase3-runtime-surface.spec.md` | 完成 | P60 MCP Phase-3 runtime surface：`mempal_phase3` 暴露 record/list/stats/gate/research_validate_plan |
| `specs/p61-runtime-adoption-recording-protocol.spec.md` | 完成 | P61 runtime adoption recording protocol：`mempal_phase3 action=guidance` 定义 used/accepted/rejected/miss/rollback 记录语义 |
| `specs/p62-runtime-adoption-cli-guidance.spec.md` | 完成 | P62 runtime adoption CLI guidance：`mempal phase3 adoption guidance` 与 MCP guidance 共用记录语义 |
| `specs/p63-runtime-adoption-record-helper.spec.md` | 完成 | P63 runtime adoption record helper：`prepare-record` / `prepare_record` 只读生成 record 命令与 payload |
| `specs/p64-runtime-adoption-record-quality-policy.spec.md` | 完成 | P64 runtime adoption record quality policy：`check-record` / `check_record` 只读检查 record 质量 |
| `specs/p65-runtime-adoption-review-report.spec.md` | 完成 | P65 runtime adoption review report：`review` 只读汇总 adoption evidence |
| `specs/p66-card-context-default-readiness.spec.md` | 完成 | P66 card context default readiness：`readiness` 只读判断 `include_cards` 是否具备未来默认开启资格 |
| `specs/p67-research-adapter-ingest.spec.md` | 完成 | P67 research adapter evidence ingest：`research-ingest-plan` 显式把 research findings 写入 evidence，candidate insights 仅作为 distill 建议 |
| `specs/p68-mcp-research-ingest-plan.spec.md` | 完成 | P68 MCP research ingest plan：`mempal_phase3 action=research_ingest_plan` 只读预览 evidence refs + distill 建议 |
| `specs/p69-runtime-adoption-checked-record.spec.md` | 完成 | P69 runtime adoption checked record：`record-checked` / `record_checked` 质量门控写入 runtime adoption evidence |
| `specs/p70-self-evolution-completion-audit.spec.md` | 完成 | P70 self-evolution completion audit：审计完整自进化 agent 系统目标，明确已完成能力与剩余缺口 |
| `specs/p71-self-evolution-loop-replay.spec.md` | 完成 | P71 self-evolution loop replay：CLI E2E replay 证明 research -> evidence -> card promotion -> context -> checked adoption 可闭环 |
| `specs/p72-runtime-adoption-capture-helper.spec.md` | 完成 | P72 runtime adoption capture helper：`capture` 把 surface/outcome 映射到 checked runtime adoption record |
| `specs/p73-evaluator-advisory-api.spec.md` | 完成 | P73 evaluator advisory API：`phase3 evaluator advise` / `mempal_phase3 action=evaluator_advise` 输出可重放 advisory 建议，不具 lifecycle authority |
| `specs/p74-card-context-default-proposal.spec.md` | 完成 | P74 card context default-on proposal：`default_proposal` 结合 P66 readiness 与 rollback criteria，生成只读默认开启提案但不改默认值 |
| `specs/p75-self-evolution-completion-audit.spec.md` | 完成 | P75 self-evolution completion audit：审计 P71-P74 后的完整自进化目标，确认 governed substrate 完成但 autonomous runtime 仍有缺口 |
| `specs/p76-spec-completeness-invariant.spec.md` | 完成 | P76 spec completeness invariant：固化每个 numbered P 必须留下 task spec 与 matching plan 的治理硬规则 |
| `specs/p77-live-adoption-instrumentation-boundary.spec.md` | 完成 | P77 live adoption instrumentation boundary：`instrumentation-policy` / `instrumentation_policy` 只读暴露 live instrumentation opt-in 边界，禁止 silent/background capture |
| `specs/p78-card-context-default-runtime-flag.spec.md` | 完成 | P78 card context default runtime flag：`context.include_cards_default` 显式默认开关 + `phase3 default-control card-context` proposal-gated enable / reversible disable |
| `specs/p79-rollback-executor-policy.spec.md` | 完成 | P79 rollback executor policy：`phase3 rollback-control card-context` / `mempal_phase3 action=rollback_control` 将 rollback evidence 转成可执行的默认关闭策略 |
| `specs/p80-autonomous-promotion-boundary-audit.spec.md` | 完成 | P80 autonomous promotion boundary audit：明确 autonomous promotion 当前出界，human-gated lifecycle authority 是最终治理边界 |
| `specs/p81-self-evolution-completion-audit.spec.md` | 完成 | P81 self-evolution completion audit：按 P80 human-gated 边界审计完整自进化 agent 系统目标并确认完成 |
| `specs/p82-opt-in-runtime-instrumentation-wrapper.spec.md` | 完成 | P82 opt-in runtime instrumentation wrapper：`mempal phase3 adoption wrap` 显式包裹一次 child command，并通过 P72/P69 checked capture 记录 runtime adoption evidence |
| `specs/p83-cognitive-brief.spec.md` | 完成 | P83 cognitive brief：`mempal brief` 生成 deterministic citation-first brief，组织 key facts / evidence / cards / uncertainty / next actions，不调用 LLM、不写 DB |
| `specs/p84-multi-agent-cowork-bus.spec.md` | 完成 | P84 multi-agent cowork bus：`agent_id` registry + per-agent inbox + CLI send/broadcast/drain/status，支持同项目多 agent 实例隔离通信 |
| `specs/p85-mcp-multi-agent-cowork-bus.spec.md` | 完成 | P85 MCP multi-agent cowork bus：`mempal_cowork_bus` 暴露 register/list/send/broadcast/drain，让 agent runtime 使用 concrete `agent_id` 总线 |
| `specs/p86-tmux-cowork-transport.spec.md` | 完成 | P86 tmux cowork transport：`transport=tmux` 显式使用 `tmux send-keys` 投递到 concrete agent pane，不经 shell、不 fallback 到 inbox |
| `specs/p87-cowork-bus-event-log.spec.md` | 完成 | P87 cowork bus event log：`events.jsonl` append-only 记录 register/send/broadcast/drain/tmux failure，并通过 `cowork-events` / `mempal_cowork_bus action=events` 回放 |
| `specs/p88-cowork-delivery-ack-status.spec.md` | 完成 | P88 cowork delivery ack/status：delivery event id 作为 `message_id`，通过 `cowork-deliveries` / `cowork-ack` 和 MCP `deliveries` / `ack` 回放 pending/drained/acked/failed |
| `specs/p89-cowork-agent-presence.spec.md` | 完成 | P89 cowork agent presence：显式 `cowork-heartbeat` / MCP `heartbeat` 更新 `last_seen_at`，`cowork-agents` / MCP `list` 推导 online/stale/never_seen |
| `specs/p90-cowork-threads-channels.spec.md` | 完成 | P90 cowork threads/channels：send/broadcast/channel_send 支持 `thread_id` / `channel` 元数据，`cowork-channel-set` / MCP `channel_set` 管理 channel membership |
| `specs/p91-tmux-live-peek.spec.md` | 完成 | P91 tmux live peek：`cowork-tmux-peek` / MCP `tmux_peek` 只读捕获已注册 tmux agent pane，不写 events/inbox/registry/DB |
| `specs/p92-multi-agent-runbook.spec.md` | 完成 | P92 multi-agent cowork runbook：`docs/COWORK-RUNBOOK.md` + `cowork-runbook` read-only CLI 固化三方/多方 agent 操作流程 |
| `specs/p93-cowork-doctor.spec.md` | 完成 | P93 cowork doctor：`cowork-doctor` / MCP `doctor` 只读诊断 registry、presence、pending deliveries、sessions、channels、可选 tmux probe |
| `specs/p94-cowork-team-session.spec.md` | 完成 | P94 cowork team session：runtime `sessions.json` + CLI/MCP session create/list/status，不写 palace.db |
| `specs/p95-cowork-handoff-summary.spec.md` | 完成 | P95 cowork handoff summary：`cowork-handoff` / MCP `handoff` 汇总 sessions、agents、pending deliveries、recent events，支持 thread/channel/session filter |
| `specs/p96-cowork-memory-capture.spec.md` | 完成 | P96 cowork memory capture：`cowork-capture` / MCP `capture` 显式把 handoff summary 写入 evidence drawer，默认 dry-run |
| `specs/p97-maintenance-runbook.spec.md` | 完成 | P97 maintenance runbook：`docs/MAINTENANCE-RUNBOOK.md` + `maintenance-runbook` read-only CLI 固化 research->evidence->knowledge/card->context/adoption/cowork capture 维护流程 |
| `specs/p98-release-install-doctor.spec.md` | 完成 | P98 release install doctor：`mempal doctor` 只读报告 binary/PATH/schema 兼容性，避免旧 binary 对新 schema 失败时无诊断 |
| `specs/p99-mcp-runtime-doctor.spec.md` | 完成 | P99 MCP runtime doctor：`mempal_doctor` 暴露 install/schema 诊断与 MCP runtime tool/action 能力清单 |
| `specs/p100-guided-maintenance-run.spec.md` | 完成 | P100 guided maintenance run：`mempal maintenance guided-run` 只读输出 dream/maintenance 推荐命令与状态计数 |
| `specs/p101-cowork-session-close-capture.spec.md` | 完成 | P101 cowork session close capture：`cowork-session-close` / MCP `session_close` 关闭 session，并可显式 dry-run/execute capture |
| `specs/p102-mcp-cognitive-brief.spec.md` | 完成 | P102 MCP cognitive brief：`mempal_brief` 向 agent runtime 暴露 deterministic citation-first brief |
| `specs/p103-adoption-analytics.spec.md` | 完成 | P103 adoption analytics：`phase3 adoption analytics` / MCP `analytics` 按 track+feature 汇总 runtime adoption evidence |
| `specs/p104-release-readiness-checklist.spec.md` | 完成 | P104 release readiness checklist：`mempal release-readiness` 只读检查 package/docs/spec-plan/runbook/doctor/schema 发布准备状态 |

### 当前 Spec（草稿，未实现）

暂无。

### 实现计划

- `docs/plans/2026-04-08-p0-implementation.md` — P0 关键路径（已完成）
- `docs/plans/2026-04-09-p1-p4-implementation.md` — P1-P4（已完成）
- `docs/plans/2026-04-11-p5-implementation.md` — P5（已完成）
- `docs/plans/2026-04-13-p6-implementation.md` — P6（已完成）
- `docs/plans/2026-04-13-p7-implementation.md` — P7（已完成）
- `docs/plans/2026-04-15-p8-implementation.md` — P8（已完成）
- `docs/plans/2026-04-17-p9-implementation.md` — P9 fact-checker + ingest-lock（已完成）
- `docs/plans/2026-04-23-p10-explicit-tunnels-implementation.md` — P10 explicit tunnels（已完成）
- `docs/plans/2026-04-23-p10-normalize-version-implementation.md` — P10 normalize-version（已完成）
- `docs/plans/2026-04-23-p11-transcript-noise-strip-implementation.md` — P11 transcript noise strip（已完成）
- `docs/plans/2026-04-24-p11-chunk-neighbors-implementation.md` — P11 chunk neighbors（已完成）
- `docs/plans/2026-04-24-p11-diary-daily-rollup-implementation.md` — P11 diary daily rollup（已完成）
- `docs/plans/2026-04-21-p12-implementation.md` — P12 mind-model bootstrap（已完成）
- `docs/plans/2026-04-23-p13a-implementation.md` — P13A wake-up statement（已完成）
- `docs/plans/2026-04-23-p13b-implementation.md` — P13B bootstrap ingest identity parity（已完成）
- `docs/plans/2026-04-24-p14-context-assembler-implementation.md` — P14 mind-model runtime context assembler（已完成）
- `docs/plans/2026-04-24-p15-mcp-context-implementation.md` — P15 mempal_context MCP tool（已完成）
- `docs/plans/2026-04-24-p16-context-skill-guidance-implementation.md` — P16 context-guided skill selection protocol（已完成）
- `docs/plans/2026-04-24-p17-knowledge-lifecycle-implementation.md` — P17 bootstrap knowledge lifecycle CLI（已完成）
- `docs/plans/2026-04-24-p18-knowledge-distill-implementation.md` — P18 bootstrap knowledge distill CLI（已完成）
- `docs/plans/2026-04-24-p19-lifecycle-ref-validation-implementation.md` — P19 lifecycle evidence ref validation（已完成）
- `docs/plans/2026-04-25-p20-promotion-gate-policy-implementation.md` — P20 promotion gate policy（已完成）
- `docs/plans/2026-04-25-p21-mcp-knowledge-gate-implementation.md` — P21 MCP knowledge gate（已完成）
- `docs/plans/2026-04-25-p22-mcp-knowledge-distill-implementation.md` — P22 MCP knowledge distill（已完成）
- `docs/plans/2026-04-26-p23-mcp-knowledge-lifecycle-implementation.md` — P23 MCP knowledge lifecycle（已完成）
- `docs/plans/2026-04-26-p24-anchor-publication-implementation.md` — P24 anchor publication CLI（已完成）
- `docs/plans/2026-04-26-p25-mcp-anchor-publication-implementation.md` — P25 MCP anchor publication（已完成）
- `docs/plans/2026-04-26-p26-dao-tian-runtime-budget-implementation.md` — P26 dao_tian runtime budget（已完成）
- `docs/plans/2026-04-26-p27-knowledge-policy-surface-implementation.md` — P27 knowledge policy surface（已完成）
- `docs/plans/2026-04-26-p28-field-taxonomy-surface-implementation.md` — P28 field taxonomy surface（已完成）
- `docs/plans/2026-04-26-p29-wake-up-context-boundary-implementation.md` — P29 wake-up/context boundary（已完成）
- `docs/plans/2026-04-27-p30-knowledge-card-storage-boundary-implementation.md` — P30 knowledge card storage boundary（已完成）
- `docs/plans/2026-04-27-p31-knowledge-card-schema-spec.md` — P31 knowledge card schema spec（已完成）
- `docs/plans/2026-04-27-p32-knowledge-card-schema-v8-implementation.md` — P32 knowledge card schema v8（已完成）
- `docs/plans/2026-04-27-p33-knowledge-card-core-api-implementation.md` — P33 knowledge card core API（已完成）
- `docs/plans/2026-04-27-p34-knowledge-card-cli-implementation.md` — P34 knowledge card CLI（已完成）
- `docs/plans/2026-04-27-p35-knowledge-card-mcp-read-implementation.md` — P35 knowledge card MCP read（已完成）
- `docs/plans/2026-04-27-p36-knowledge-card-backfill-report-implementation.md` — P36 knowledge card backfill report（已完成）
- `docs/plans/2026-04-27-p37-knowledge-card-backfill-apply-implementation.md` — P37 knowledge card backfill apply（已完成）
- `docs/plans/2026-04-27-p38-p42-knowledge-card-runtime-implementation.md` — P38-P42 knowledge card runtime baseline（已完成）
- `docs/plans/2026-04-28-p43-knowledge-card-retrieval-contract.md` — P43 knowledge card retrieval contract（已完成）
- `docs/plans/2026-04-28-p44-card-context-assembler.md` — P44 card-aware context assembler（已完成）
- `docs/plans/2026-04-28-p45-card-linked-evidence-retrieval.md` — P45 card linked-evidence retrieval（已完成）
- `docs/plans/2026-04-28-p46-card-context-default-policy.md` — P46 card context default policy（已完成）
- `docs/plans/2026-04-28-p47-card-embedding-policy.md` — P47 card embedding policy（已完成）
- `docs/plans/2026-04-29-p48-card-audit-policy.md` — P48 card audit policy（已完成）
- `docs/plans/2026-04-29-p49-research-ingestion-policy.md` — P49 research ingestion policy（已完成）
- `docs/plans/2026-04-29-p50-evaluator-promotion-policy.md` — P50 evaluator promotion policy（已完成）
- `docs/plans/2026-04-29-p51-mind-model-closure-audit.md` — P51 mind model closure audit（已完成）
- `docs/plans/2026-05-01-p52-phase-3-intake-roadmap.md` — P52 phase 3 intake roadmap（已完成）
- `docs/plans/2026-05-02-p53-phase-3-candidate-evidence-audit.md` — P53 phase 3 candidate evidence audit（已完成）
- `docs/plans/2026-05-02-p54-runtime-adoption-evidence.md` — P54 runtime adoption evidence（已完成）
- `docs/plans/2026-05-02-p55-runtime-adoption-cli.md` — P55 runtime adoption CLI（已完成）
- `docs/plans/2026-05-02-p56-card-context-default-gate.md` — P56 card context default gate（已完成）
- `docs/plans/2026-05-02-p57-card-embedding-evidence-gate.md` — P57 card embedding evidence gate（已完成）
- `docs/plans/2026-05-02-p58-evaluator-api-evidence-gate.md` — P58 evaluator API evidence gate（已完成）
- `docs/plans/2026-05-02-p59-research-adapter-ingestion-contract.md` — P59 research adapter ingestion contract（已完成）
- `docs/plans/2026-05-05-p60-mcp-phase3-runtime-surface.md` — P60 MCP Phase-3 runtime surface（已完成）
- `docs/plans/2026-05-10-p61-runtime-adoption-recording-protocol.md` — P61 runtime adoption recording protocol（已完成）
- `docs/plans/2026-05-12-p62-runtime-adoption-cli-guidance.md` — P62 runtime adoption CLI guidance（已完成）
- `docs/plans/2026-05-12-p63-runtime-adoption-record-helper.md` — P63 runtime adoption record helper（已完成）
- `docs/plans/2026-05-13-p64-runtime-adoption-record-quality-policy.md` — P64 runtime adoption record quality policy（已完成）
- `docs/plans/2026-05-13-p65-runtime-adoption-review-report.md` — P65 runtime adoption review report（已完成）
- `docs/plans/2026-05-13-p66-card-context-default-readiness.md` — P66 card context default readiness（已完成）
- `docs/plans/2026-05-13-p67-research-adapter-ingest.md` — P67 research adapter evidence ingest（已完成）
- `docs/plans/2026-05-13-p68-mcp-research-ingest-plan.md` — P68 MCP research ingest plan（已完成）
- `docs/plans/2026-05-13-p69-runtime-adoption-checked-record.md` — P69 runtime adoption checked record（已完成）
- `docs/plans/2026-05-13-p70-self-evolution-completion-audit.md` — P70 self-evolution completion audit（已完成）
- `docs/plans/2026-05-13-p71-self-evolution-loop-replay.md` — P71 self-evolution loop replay（已完成）
- `docs/plans/2026-05-13-p72-runtime-adoption-capture-helper.md` — P72 runtime adoption capture helper（已完成）
- `docs/plans/2026-05-13-p73-evaluator-advisory-api.md` — P73 evaluator advisory API（已完成）
- `docs/plans/2026-05-13-p74-card-context-default-proposal.md` — P74 card context default-on proposal（已完成）
- `docs/plans/2026-05-13-p75-self-evolution-completion-audit.md` — P75 self-evolution completion audit（已完成）
- `docs/plans/2026-05-13-p76-spec-completeness-invariant.md` — P76 spec completeness invariant（已完成）
- `docs/plans/2026-05-13-p77-live-adoption-instrumentation-boundary.md` — P77 live adoption instrumentation boundary（已完成）
- `docs/plans/2026-05-13-p78-card-context-default-runtime-flag.md` — P78 card context default runtime flag（已完成）
- `docs/plans/2026-05-13-p79-rollback-executor-policy.md` — P79 rollback executor policy（已完成）
- `docs/plans/2026-05-13-p80-autonomous-promotion-boundary-audit.md` — P80 autonomous promotion boundary audit（已完成）
- `docs/plans/2026-05-13-p81-self-evolution-completion-audit.md` — P81 self-evolution completion audit（已完成）
- `docs/plans/2026-05-25-p82-opt-in-runtime-instrumentation-wrapper.md` — P82 opt-in runtime instrumentation wrapper（已完成）
- `docs/plans/2026-05-25-p83-cognitive-brief.md` — P83 cognitive brief（已完成）
- `docs/plans/2026-05-25-p84-multi-agent-cowork-bus.md` — P84 multi-agent cowork bus（已完成）
- `docs/plans/2026-05-25-p85-mcp-multi-agent-cowork-bus.md` — P85 MCP multi-agent cowork bus（已完成）
- `docs/plans/2026-05-25-p86-tmux-cowork-transport.md` — P86 tmux cowork transport（已完成）
- `docs/plans/2026-05-25-p87-cowork-bus-event-log.md` — P87 cowork bus event log（已完成）
- `docs/plans/2026-05-25-p88-cowork-delivery-ack-status.md` — P88 cowork delivery ack/status（已完成）
- `docs/plans/2026-05-25-p89-cowork-agent-presence.md` — P89 cowork agent presence（已完成）
- `docs/plans/2026-05-25-p90-cowork-threads-channels.md` — P90 cowork threads/channels（已完成）
- `docs/plans/2026-05-25-p91-tmux-live-peek.md` — P91 tmux live peek（已完成）
- `docs/plans/2026-05-26-p92-multi-agent-runbook.md` — P92 multi-agent cowork runbook（已完成）
- `docs/plans/2026-05-26-p93-cowork-doctor.md` — P93 cowork doctor（已完成）
- `docs/plans/2026-05-26-p94-cowork-team-session.md` — P94 cowork team session（已完成）
- `docs/plans/2026-05-26-p95-cowork-handoff-summary.md` — P95 cowork handoff summary（已完成）
- `docs/plans/2026-05-26-p96-cowork-memory-capture.md` — P96 cowork memory capture（已完成）
- `docs/plans/2026-05-26-p97-maintenance-runbook.md` — P97 maintenance runbook（已完成）
- `docs/plans/2026-05-26-p98-release-install-doctor.md` — P98 release install doctor（已完成）
- `docs/plans/2026-05-26-p99-mcp-runtime-doctor.md` — P99 MCP runtime doctor（已完成）
- `docs/plans/2026-05-26-p100-guided-maintenance-run.md` — P100 guided maintenance run（已完成）
- `docs/plans/2026-05-26-p101-cowork-session-close-capture.md` — P101 cowork session close capture（已完成）
- `docs/plans/2026-05-26-p102-mcp-cognitive-brief.md` — P102 MCP cognitive brief（已完成）
- `docs/plans/2026-05-26-p103-adoption-analytics.md` — P103 adoption analytics（已完成）
- `docs/plans/2026-05-26-p104-release-readiness-checklist.md` — P104 release readiness checklist（已完成）

### Spec 使用方式

```bash
agent-spec parse specs/p6-cowork-peek-and-decide.spec.md
agent-spec lint specs/p6-cowork-peek-and-decide.spec.md --min-score 0.7
```

## 关键架构约束

- **存储**：SQLite + sqlite-vec，单文件 `~/.mempal/palace.db`，schema v9
- **嵌入**：model2vec-rs 默认（potion-multilingual-128M, 256d），可选 ort (ONNX) 通过 `onnx` feature flag
- **搜索**：BM25 (FTS5) + 向量 + RRF 融合混合检索
- **AAAK 是输出格式化器**：不被 ingest 或 search 依赖
- **数据永远 raw 存储**：drawers 表存原文，向量索引在 drawer_vectors 表（维度动态）
- **搜索结果强制带引用**：`SearchResult` 包含 `source_file`、`drawer_id`、`tunnel_hints`
- **知识图谱**：triples 表已激活（手动 CRUD），支持时态验证
- **隧道**：动态跨 Wing 链接发现，内联到搜索结果
- **自描述协议**：MEMORY_PROTOCOL 嵌入 MCP ServerInfo.instructions，17 条规则

## MCP 工具（23 个）

| 工具 | 作用 |
|------|------|
| `mempal_status` | 状态 + 协议 + AAAK spec |
| `mempal_doctor` | release/install + MCP runtime diagnostics：报告 binary/PATH/schema 兼容性和 required MCP tool/action 能力清单（P99） |
| `mempal_search` | 混合检索（BM25 + 向量 + RRF + tunnel hints）+ AAAK 结构化 signals（P7） |
| `mempal_context` | mind-model runtime context：按 `dao_tian -> dao_ren -> shu -> qi` 组装指导性 context pack；`dao_tian_limit` 默认 1；用于辅助 workflow/skill/tool 选择但不自动执行（P15/P16/P26/P44） |
| `mempal_brief` | deterministic citation-first cognitive brief：summary/key facts/evidence/cards/uncertainty/next actions，不调用 LLM、不写 DB（P102） |
| `mempal_field_taxonomy` | read-only Stage-1 field taxonomy guidance：推荐 `field` 值但不限制自定义字段（P28） |
| `mempal_knowledge_distill` | 从 existing evidence drawer refs 创建 candidate `dao_ren` / `qi` knowledge drawer（P22） |
| `mempal_knowledge_policy` | read-only Stage-1 promotion policy：列出 `dao_tian/dao_ren/shu/qi` 提升阈值（P27） |
| `mempal_knowledge_gate` | read-only promotion readiness check：评估 knowledge drawer 是否满足提升门槛（P21） |
| `mempal_knowledge_promote` | gate-enforced knowledge lifecycle promotion（P23） |
| `mempal_knowledge_demote` | evidence-backed knowledge demotion / retirement（P23） |
| `mempal_knowledge_publish_anchor` | metadata-only outward anchor publication（P25） |
| `mempal_knowledge_cards` | Phase-2 knowledge card list/get/events/gate/promote/demote/retrieve；retrieve 通过 linked evidence 返回 active cards（P35/P40/P45） |
| `mempal_phase3` | Phase-3 runtime adoption evidence：guidance/instrumentation_policy/prepare_record/capture/evaluator_advise/default_proposal/check_record/record_checked/review/readiness/analytics/record/list/stats/gate/research_validate_plan/research_ingest_plan（P60/P61/P63/P64/P65/P66/P68/P69/P72/P73/P74/P77/P103） |
| `mempal_ingest` | 写记忆（支持 dry_run；P9-B 暴露 `lock_wait_ms`） |
| `mempal_delete` | soft-delete（+ audit） |
| `mempal_taxonomy` | Wing/Room 路由关键词管理 |
| `mempal_kg` | 知识图谱三元组（add/query/invalidate） |
| `mempal_tunnels` | 跨 Wing 链接发现 |
| `mempal_peek_partner` | 读 partner agent 当前 session（live，不存储） |
| `mempal_cowork_push` | 主动投递 ephemeral handoff 到 partner inbox（at-next-submit 交付） |
| `mempal_cowork_bus` | 多 agent concrete `agent_id` 总线：register/list/send/broadcast/drain/events/deliveries/ack/heartbeat/channel_set/channel_list/channel_send/tmux_peek/doctor/session_create/session_list/session_status/session_close/handoff/capture，支持 opt-in `transport=tmux`、event replay、delivery ack/status、presence、group channels、read-only tmux pane peek、诊断、session、handoff summary、显式 handoff-to-evidence capture（P85/P86/P87/P88/P89/P90/P91/P93/P94/P95/P96/P101） |
| `mempal_fact_check` | 离线矛盾检测（SimilarNameConflict / RelationContradiction / StaleFact）—— P9 |

## mempal 检索纪律

当 agent 回答本项目的历史决策、实现细节、bug 成因、架构理由、或“为什么/怎么工作”类问题时：

1. 每个 session 先调一次 `mempal_status`，再决定是否使用 `wing` / `room` filter。
2. 对项目事实优先使用 `mempal_search`，不要只靠 repo grep、当前对话记忆或常识猜测。
3. 在本仓库内，只要 `mempal_status` 已确认存在 `wing="mempal"`，默认先用 `wing="mempal"` 缩小范围；只有用户明确要求跨项目 / 全局搜索时才放宽。
4. 历史决策、设计理由、bug 成因这类问题，第一轮检索默认使用简短英文语义 query，且 `top_k=2`；只有证据不足时才逐步放大到 `top_k=3-4` 或放宽 query。
5. 如果 MCP 客户端提示 Large response / Large MCP response，优先重试更窄的 query、加 `wing` / `room`、或降低 `top_k`；不要直接消费一大段 raw `content`。
6. 显式消费 `mempal_search` 返回的结构化 signals，而不是只读 `content`：
   - 决策问题：优先 `flags` 包含 `DECISION` 的结果
   - 实现 / bug / 架构问题：优先 `flags` 包含 `TECHNICAL` 的结果
   - 同等条件下优先处理 `importance_stars` 更高的结果
   - 用 `entities` 和 `topics` 缩小歧义结果集
7. 将 `content` 视为 raw text；不要期待或解析 `mempal_search` 返回 AAAK 格式文本。
8. 基于 mempal 结果作答时，必须引用 `drawer_id` 和 `source_file`。
9. 如果没有找到高信号结果，要明确说明“没找到足够证据”，然后扩大查询范围；不要猜。

## Workspace 结构

```
crates/
├── mempal-core/      # 数据模型 + SQLite schema v9 + taxonomy + triples
├── mempal-ingest/    # 导入管道
├── mempal-search/    # 混合搜索（BM25+向量+RRF）+ 路由 + tunnel hints
├── mempal-embed/     # 嵌入层（model2vec 默认, ort 可选）
├── mempal-aaak/      # AAAK 编解码（输出侧）
├── mempal-mcp/       # MCP 服务器（9 工具）
├── mempal-api/       # REST API（feature-gated）
└── mempal-cli/       # CLI 入口（含 reindex, kg, tunnels）
```

## 代码规范

- Edition 2024
- `#![warn(clippy::all)]`
- 错误处理：`anyhow`（应用层）+ `thiserror`（库层）
- 异步：`tokio`，features=["full"]
- 不用 `.unwrap()`，用 `?` 或 `.expect("reason")`
- 每次 commit 后：调 `mempal_ingest` 存决策记忆（Rule 4）

## Auto-Dream 集成

当 Claude Code 执行 auto-dream 或手动 dream 时：

1. **验证**：调 `mempal_search` 核实正在整理的事实是否与 mempal 记忆一致
2. **保存**：将整理后的高价值洞察存到 mempal（`mempal_ingest`，importance >= 3）
3. **解矛盾**：MEMORY.md 与 mempal drawer 矛盾时，以 mempal 为准（mempal 有出处引用）
4. **写日记**：将 dream 摘要存为 agent diary（`wing="agent-diary"`, `room="claude"`）
5. **清理 KG**：检查 triples 中是否有过期关系需要 invalidate

Dream 是 mempal 的"REM 睡眠"——短期 session 记忆被整理为长期项目记忆。

## Known Limitations / Operational Notes

以下是 0.3.0 已知、来自真实 E2E 的跨系统约束，和 mempal 代码无关但会直接影响使用体验，记录在此避免重复踩坑：

1. **`mempal cowork-install-hooks` 写两件 Claude 侧制品**：`.claude/hooks/user-prompt-submit.sh`（脚本）+ `.claude/settings.json` 下的 `hooks.UserPromptSubmit` 条目（注册）。Claude Code 不按文件名约定自动发现脚本，两件**都必须有**hook 才会 fire。`install-hooks` 已自动处理 + 自愈 stale 条目；请勿手工移除其中任一。
2. **Codex 侧依赖 `codex_hooks` feature flag**：shipped `codex-cli ≤ 0.120.0` 该 flag 处于 "under development" 且默认 `false`，此时 Codex runtime 完全忽略 `~/.codex/hooks.json`。`install-hooks` 检测到会打印 warning + 激活命令 `codex features enable codex_hooks`。
3. **Codex TUI 进程启动时一次性缓存 config**：改完 `config.toml` 或 `hooks.json`（含 feature flag / install-hooks）后，必须完全退出并重启 Codex TUI；已在跑的进程拿不到新配置。
4. **Claude Code 的 MCP server 是 session startup spawn**：`cargo install` 升级 mempal binary 后，Claude Code 还在用旧 MCP server 进程，**不认识新加的工具**（如 `mempal_cowork_push`）。升级后重启 Claude Code，MCP server 会 respawn 到新 binary。
5. **`mempal_cowork_push` 依赖 MCP `ClientInfo.name` 被识别为 Claude/Codex 家族之一**（`src/cowork/peek.rs` `Tool::from_str_ci`）：caller_tool 推断基于 MCP `ClientInfo.name`，是 self-push 拒绝和 `InboxMessage.from` 填写的前提。当前识别名单（0.3.1）：`claude` / `claude-code` / `claude_code` / `codex` / `codex-cli` / `codex_cli` / `codex-tui` / `codex-mcp-client`（Codex 实际发送的字符串，源自 `codex-rs/codex-mcp/src/mcp_connection_manager.rs:1458`）。其它 MCP 客户端名字不在此列表时即使显式传 `target_tool` 也会被拒；这不是 by-design scope 限制，只是当前只覆盖 Claude↔Codex pair，遇到新家族继续扩名单即可。
