# Multi-Agent Cowork Runbook

This runbook describes the operational workflow for using mempal with more
than two live agent instances in the same project.

## 1. Register Concrete Agents

Every live participant gets a stable `agent_id`.

```bash
mempal cowork-register --cwd "$PWD" --agent-id claude-main --tool claude
mempal cowork-register --cwd "$PWD" --agent-id codex-a --tool codex
mempal cowork-register --cwd "$PWD" --agent-id codex-b --tool codex
```

For tmux-backed agents, register the pane explicitly:

```bash
mempal cowork-register \
  --cwd "$PWD" \
  --agent-id codex-a \
  --tool codex \
  --transport tmux \
  --tmux-target mempal:0.1
```

## 2. Inspect Presence And Inbox State

```bash
mempal cowork-agents --cwd "$PWD"
mempal cowork-heartbeat --cwd "$PWD" --agent-id codex-a
```

Presence is explicit. mempal does not run a daemon and does not infer liveness
unless an agent reports a heartbeat or is re-registered.

## 3. Send Direct Messages

```bash
mempal cowork-send \
  --cwd "$PWD" \
  --from claude-main \
  --to codex-a \
  --thread-id p92-review \
  --message "Please review the doctor design."
```

Inbox agents receive JSONL inbox messages. tmux agents receive direct pane
delivery through `tmux send-keys`.

## 4. Broadcast Or Use Channels

For ad hoc fanout:

```bash
mempal cowork-broadcast \
  --cwd "$PWD" \
  --from claude-main \
  --to codex-a \
  --to codex-b \
  --message "Sync point: review P93 tests."
```

For stable groups:

```bash
mempal cowork-channel-set \
  --cwd "$PWD" \
  --channel review \
  --agent codex-a \
  --agent codex-b

mempal cowork-channel-send \
  --cwd "$PWD" \
  --from claude-main \
  --channel review \
  --thread-id p92-review \
  --message "Review the current patch."
```

## 5. Drain, Ack, And Audit

```bash
mempal cowork-agent-drain --cwd "$PWD" --agent-id codex-a
mempal cowork-deliveries --cwd "$PWD" --agent-id codex-a
mempal cowork-ack --cwd "$PWD" --agent-id codex-a --message-id evt-...
mempal cowork-events --cwd "$PWD" --limit 20
```

`cowork-events` is an operational event log. It is not durable memory and it
does not replace explicit ingest.

## 6. Use tmux Live Peek

```bash
mempal cowork-tmux-peek --cwd "$PWD" --agent-id codex-a --lines 80
```

Peek is read-only. It does not write events, inboxes, sessions, or `palace.db`.

## 7. Diagnose Before Escalating

```bash
mempal cowork-doctor --cwd "$PWD"
mempal cowork-doctor --cwd "$PWD" --probe-tmux --format json
```

Doctor reports registry, stale presence, pending deliveries, channels,
sessions, and optional tmux reachability.

## 8. Create Sessions And Handoffs

```bash
mempal cowork-session-create \
  --cwd "$PWD" \
  --session-id p92-review \
  --title "P92-P97 implementation" \
  --agent claude-main \
  --agent codex-a \
  --thread-id p92-review \
  --goal "Finish specs, implementation, and verification."

mempal cowork-handoff --cwd "$PWD" --session-id p92-review
```

Sessions and handoffs are runtime coordination artifacts. They do not write
durable project memory unless explicitly captured.

## 9. Explicitly Capture Durable Memory

```bash
mempal cowork-capture \
  --cwd "$PWD" \
  --summary-source handoff \
  --session-id p92-review \
  --execute \
  --format json
```

Capture is the intentional bridge from runtime operations to evidence memory.
It is never automatic.
