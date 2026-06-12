# AgentHouse-RS Architecture

## Product Shape

AgentHouse-RS `0.1.0` is a macOS-native GPUI workspace for humans and local agents.

The product hierarchy is:

```text
Workspace
  Window
    Tab
      Session, when the tab runs a terminal or browser runtime
        Blocks
        Notification ring
```

A `Workspace` is a folder-backed project container. A `Window` is an AgentHouse work surface inside a workspace, not an operating-system window. A `Tab` is the active surface within that window, such as a terminal tab or a browser tab. A terminal tab owns a PTY-backed session. A browser tab owns a native WKWebView-backed session.

The durable collaboration object is a `Block`: a structured unit of work produced by a terminal command, external agent process, user action, or system event.

## Agent Control Plane

AgentHouse exposes a local Unix-domain JSONL socket so agents can operate the same workspace model as humans:

```text
one ControlRequest JSON object per line
one ControlResponse JSON object per line
```

The control plane currently covers:

- inspection: `snapshot`, `list_workspaces`, `list_windows`, `list_window_tabs`, `list_sessions`, `list_events`;
- workspace/window actions: create/activate workspace, create/activate/close windows, move/close/activate tabs, set layout;
- targeted 0.1.0 actions: `open_terminal_window`, `open_terminal_tab`, `open_web_window`, `open_web_tab`, `split_window`;
- terminal actions: run command, write input, send semantic key, resize, interrupt, terminate, restart;
- browser actions: navigate, action, input, resize, capture browser surface;
- collaboration actions: acknowledge session ring, list blocks, forward blocks;
- observation: `capture_surface` and `capture_session_surface` structured snapshots, with screenshots when the platform permits.

This is intentionally local-only. It is not a network API and does not yet include capability tokens, remote auth, or multi-user policy.

## Crate Boundaries

```text
ah-core
  Shared IDs, timestamps, and base types.

ah-workspace
  Workspace, window, tab, and layout metadata.

ah-session
  Session lifecycle metadata. It does not own PTY, UI, or storage.

ah-terminal
  PTY process lifecycle and terminal grid parsing.

ah-block
  Block model, references, state transitions, and forwarding payloads.

ah-web
  Browser runtime trait and serializable browser state.

ah-webview-macos
  macOS WKWebView provider used by the Alpha browser path.

ah-store
  SQLite persistence boundary.

ah-control
  JSON-serializable local control protocol.

ah-use
  External harness that validates the product through the control socket.

ah-app
  GPUI application composition and user-facing shell.
```

## Browser Strategy

The `0.1.0` browser path is macOS WKWebView through `ah-webview-macos`. The product contract is a browser tab that can navigate, report URL/title/load state, provide structured surface state, and coexist with terminal tabs in the same workspace/window model.

Current limitation: the Alpha path validates native WKWebView loading and frame capture through the GPUI/control route, but full interactive embedded NSView behavior still needs stricter product validation before it should be marketed as complete browser embedding.

## Terminal Strategy

`ah-terminal` owns process and PTY lifecycle:

- spawn configured shells or external agent commands;
- emit output chunks, exit events, resize acknowledgements, and write failures;
- maintain an Alacritty-backed terminal screen snapshot for structured observation;
- keep UI rendering in `ah-app`.

Terminal sessions are the first agent runtime. External agents run as processes, and future detection layers should consume terminal/session/block events instead of bypassing the terminal path.

## Release Boundary

`0.1.0` deliberately excludes Servo, fingerprint-browser behavior, FlowBoard, cross-platform ports, file/folder browser scope, and network-exposed control APIs. Those should live on separate branches or future milestones rather than leaking into the Alpha release branch.
