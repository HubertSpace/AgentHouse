# AgentHouse Local Control Guide

This document describes the local control surface included in the `0.1.0` Beta.
It is a developer and smoke-test interface, not a remote automation product.

Humans use the GPUI shell. Local tools can inspect the same workspace model
through a Unix-domain JSONL socket.

## Boundary

The Beta control surface is intentionally narrow:

- inspect the running app;
- create and activate workspaces;
- open terminal and browser tabs;
- split and activate work surfaces;
- drive terminal sessions;
- capture structured surface state for smoke tests.

It is local-only. It is not authenticated, not network-exposed, and not a
production remote-control API.

## Transport

The first control transport is a local Unix-domain socket:

```text
${TMPDIR}/agenthouse.sock
```

Requests and responses are JSON Lines:

```text
one ControlRequest JSON object per line
one ControlResponse JSON object per line
```

`watch_events` is the streaming exception: it keeps the socket open and emits
one stream message JSON object per line.

## Product Model

```text
Workspace
  Window
    Pane
      Tab
        Session
          Blocks
```

A workspace is the top-level working context. A window is an AgentHouse work
surface inside a workspace, not an operating-system window. A pane is a split
area. A tab is a switchable surface such as terminal or browser. A session is
the runtime behind a tab.

Blocks are structured records produced by sessions. In the Beta they are mainly
used for terminal command output, observation, and smoke-test verification.

## Public Beta Actions

Inspection:

- `ping`
- `snapshot`
- `get_app_settings`
- `list_workspaces`
- `list_windows`
- `list_window_tabs`
- `list_sessions`
- `list_events`

Workspace and surface control:

- `create_workspace`
- `activate_workspace`
- `create_window`
- `activate_window`
- `close_window`
- `split_window`
- `activate_window_tab`
- `move_window_tab`
- `close_window_tab`
- `open_terminal_window`
- `open_terminal_tab`
- `open_web_window`
- `open_web_tab`

Terminal session control:

- `run_terminal_command`
- `write_terminal_input`
- `send_terminal_key`
- `resize_terminal`
- `interrupt_session`
- `terminate_session`
- `restart_session`

Observation:

- `capture_surface`
- `capture_session_surface`
- `watch_events`
- `list_blocks`

Settings:

- `set_ui_language` with `zh-cn` or `en`
- `set_ui_theme_scheme` with `cream`, `warm`, `blue`, `green`, `red`, `purple`,
  `glass`, `luxury`, or `soft`

Browser actions are conservative in `0.1.0`. Browser tabs support the native
macOS WKWebView product path and should be validated through app-path smoke
tests before being described as supported user workflows.

## CLI Harness

`ah-use` is the local control client used by smoke tests.

Basic checks:

```sh
cargo run -p ah-use -- ping
cargo run -p ah-use -- inspect snapshot
cargo run -p ah-use -- inspect settings
```

Product-path smoke scenarios:

```sh
cargo run -p ah-use -- platform-loop
cargo run -p ah-use -- window-api-loop
cargo run -p ah-use -- session-lifecycle
```

For isolated validation, run the app and harness with the same `TMPDIR`:

```sh
mkdir -p /tmp/agenthouse-smoke
TMPDIR=/tmp/agenthouse-smoke cargo run -p agenthouse
TMPDIR=/tmp/agenthouse-smoke cargo run -p ah-use -- platform-loop
```

## Operating Rules

Use bounded waits and retries. Commands are asynchronous, so tests should wait
for events, blocks, or terminal-tail text instead of assuming immediate
completion.

Read `snapshot` before destructive actions, and verify IDs are still present
before sending follow-up commands.

Use screenshots and structured surface captures as evidence for smoke tests and
debugging. They are not a guarantee that browser automation is complete.
