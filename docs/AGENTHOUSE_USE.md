# AgentHouse-use Control Guide

AgentHouse is a human and agent shared workspace. Humans use the GPUI shell; agents use the same
workspace, window, tab, session, block, and notification-ring model through the local control plane.

## Transport

The first control transport is a local Unix domain socket:

```text
${TMPDIR}/agenthouse.sock
```

Requests and responses are JSON Lines. A client writes one `ControlRequest` per line and receives
one `ControlResponse` per line. `watch_events` is the streaming exception: it keeps the socket open
and emits one `ControlStreamMessage` JSON object per line.

This is intentionally local-only. AgentHouse can run commands and expose screenshots, so remote
transport should wait for capability tokens, approval policy, and audit controls.

## Product Model

```text
Workspace
  Window
    Tab
      Session, when the tab runs a terminal or external agent
        Blocks
        Notification ring
      Preview, when the tab shows web or local file state
```

A workspace is the top-level working context. A window is an AgentHouse surface inside a workspace,
not an operating-system window. A window can hold multiple tabs. Terminal tabs own sessions. Web and
file tabs are previews unless an agent is attached later.

Sessions are lifecycle-controlled runtime objects. They can run commands, accept terminal input,
resize, interrupt, terminate, restart, emit blocks, and raise notification-ring state. Blocks are the
durable exchange unit for handoff between humans, agents, and other sessions.

## Observation Layers

Agents should observe AgentHouse through three layers instead of depending on only one signal.

Events and logs:

- `list_events` returns bounded recent runtime events.
- `watch_events` streams events plus heartbeat/error messages.
- Event topics include `app`, `store`, `workspace`, `window`, `session`, and `terminal`.

Structured API state:

- `snapshot` returns active workspace, all workspaces, windows, and sessions.
- `get_app_settings` returns the current UI language, color scheme, fixed light mode, and available
  options.
- `list_workspaces`, `list_windows`, `list_window_tabs`, `list_sessions`, and `list_blocks` expose
  scoped state.
- `get_session` exposes status, block count, ring state, ring summary, and unread count.
- Block summaries include text plus attachment metadata. Claude stream-json cleanup preserves the
  raw JSONL as a file attachment when possible.

Surface and screenshot state:

- `capture_surface` captures the active or selected window.
- `capture_session_surface` captures the window that owns a session.
- Surface captures include active ids, content type, URL/path/session summaries, recent blocks,
  terminal tail, a structured snapshot path, and a PNG screenshot path when the OS allows it.

The intended agent pattern is: watch events for wakeups, inspect API state for decisions, and use
surface capture for visual verification or recovery.

## Actions

Workspace and window actions:

- `set_ui_language` with `zh-cn` or `en`
- `set_ui_theme_scheme` with `cream`, `warm`, `blue`, `green`, `red`, `purple`, `glass`, `luxury`,
  or `soft`
- `create_workspace`
- `activate_workspace`
- `create_window`
- `activate_window`
- `set_workspace_layout` with `single`, `columns`, or `grid`
- `open_terminal_window`
- `open_web_window`
- `open_file_preview`
- `activate_window_tab`
- `move_window_tab`
- `close_window_tab`
- `close_window`

Session actions:

- `run_terminal_command`
- `write_terminal_input`
- `send_terminal_key`
- `resize_terminal`
- `interrupt_session`
- `terminate_session`
- `restart_session`
- `ack_session_ring`

Block actions:

- `list_blocks`
- `forward_block`

Agents should treat command execution as asynchronous. Queue a command, then wait for either a
completed block, a session ring transition, an event topic, or terminal-tail text.

## CLI Harness

`ah-use` is the first AgentHouse-use client. It only talks through the control socket.

Examples:

```sh
cargo run -p ah-use -- ping
cargo run -p ah-use -- inspect snapshot
cargo run -p ah-use -- inspect settings
cargo run -p ah-use -- act set-language en
cargo run -p ah-use -- act set-theme glass
cargo run -p ah-use -- act open-terminal <workspace-id> "Agent terminal"
cargo run -p ah-use -- act key <session-id> up
cargo run -p ah-use -- wait block <session-id> AH_MARKER 30
cargo run -p ah-use -- watch 20
```

Scenario commands:

- `claude-tui-smoke`: starts the real Claude Code TUI through raw terminal input and verifies that
  AgentHouse can observe its interactive screen.
- `claude-tui-key-smoke`: starts the real Claude Code TUI, confirms the trust prompt with semantic
  terminal keys when it appears, and verifies the post-confirmation screen.
- `claude-tui-prompt-smoke`: starts the real Claude Code TUI, pastes a short prompt into the TUI,
  sends Enter through the semantic key API, and waits for the response marker in the terminal
  surface snapshot.
- `platform-loop`: verifies workspace, targeted terminal tab, targeted native web tab, split,
  ring, event, and surface APIs.
- `window-api-loop`: verifies multi-window, tab move/close, and layout controls.
- `session-lifecycle`: verifies write, interrupt, terminate, restart, resize, blocks, and surface.
- `interactive-stdin`: verifies an interactive command can receive stdin and complete through the
  same terminal session.
- `terminal-grid-loop`: verifies ANSI terminal output is rendered through the Alacritty-backed
  screen grid and exposed in structured surface snapshots.
- `terminal-key-loop`: verifies agents can send semantic terminal keys such as arrows, enter, tab,
  and control chords without hand-writing escape bytes.
- `terminal-tui-loop`: verifies a live alternate-screen TUI can be observed through structured
  terminal snapshots, receive raw keyboard input, and exit back to the shell.
- `terminal-paste-loop`: verifies bracketed paste reaches a raw terminal process intact through the
  control API.
- `watch-loop`: verifies streaming event observation.
- `handoff-loop`: forwards a completed block between sessions.
- `debate-loop`: runs two Claude sessions and forwards completed debate blocks between them.
- `stress-loop [seconds]`: repeatedly drives layouts, commands, block waits, forwarding, ring ack,
  event reads, snapshots, and surface captures. The default is 1800 seconds.

For isolated stress validation, run the app and harness with the same `TMPDIR`:

```sh
mkdir -p /tmp/agenthouse-rs-stress
TMPDIR=/tmp/agenthouse-rs-stress cargo run -p ah-app
TMPDIR=/tmp/agenthouse-rs-stress cargo run -p ah-use -- stress-loop 1800
```

## Library Harness

`ah-use` also exposes `ControlClient` and `AhUseAgent` for agent implementations. `ControlClient`
is the raw JSONL request/streaming client. `AhUseAgent` wraps common actions and waits:

- `snapshot`, `list_events`, `capture_surface`, `inspect_session_surface`
- `create_workspace`, `activate_workspace`, `create_window`, `activate_window`, `list_windows`
- `open_terminal`, `open_terminal_tab`, `open_web`, `open_web_tab`, `split_window`,
  `set_workspace_layout`
- `act_run_shell_command`, `act_write_terminal_input`, `act_send_terminal_key`,
  `act_forward_block`
- `act_ack_ring`, `act_interrupt_session`, `act_terminate_session`, `act_restart_session`
- `wait_block`, `wait_block_with_timeout`, `wait_ring_state`, `wait_event_topic`

## Agent Operating Rules

Use bounded waits and retries. Every command should have an external marker so a block or terminal
tail can be matched deterministically.

Prefer idempotent control sequences. Read `snapshot` before destructive actions, and verify ids are
still present before sending commands.

Use the notification ring as a wakeup signal, not as the only source of truth. After a ring changes,
read blocks or surface state before acting.

Keep screenshots and raw attachments as evidence. They are useful for human review and for recovery
when event/API state and visual state diverge.

Do not assume a web tab is fully browser-automation complete until the specific action has been
validated through `BrowserAction` or `SendBrowserInput`. In `0.1.0`, web tabs use the native macOS
WKWebView path; browser-engine experiments are outside the Alpha release branch.
