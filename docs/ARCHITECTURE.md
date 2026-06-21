# AgentHouse Architecture

## Product Shape

AgentHouse `0.1.0` is a macOS-native GPUI workspace for humans and local AI
agents.

The product hierarchy is:

```text
Workspace
  Window
    Pane
      Tab
        Session
          Blocks
```

A `Workspace` is a folder-backed working context. A `Window` is an AgentHouse
work surface inside a workspace, not an operating-system window. A `Pane` is a
split area in that surface. A `Tab` is the active tool or view inside a pane,
such as a terminal or browser tab. A `Session` is the runtime behind a tab.

The durable collaboration object is a `Block`: a structured unit of work or
evidence produced by a terminal command, an agent process, a browser event, a
user action, or a system event.

## Local Control Plane

AgentHouse exposes a local Unix-domain JSONL socket so local agents and smoke
tests can inspect and operate the same workspace model as the user:

```text
one ControlRequest JSON object per line
one ControlResponse JSON object per line
```

The public Beta control boundary covers:

- inspection: snapshot, workspaces, windows, tabs, sessions, events, settings;
- workspace and window actions: create, activate, close, split, move, and set
  layout;
- terminal actions: run command, write input, send semantic key, resize,
  interrupt, terminate, and restart;
- browser actions used by the app path and smoke tests: open, navigate, resize,
  and capture surface;
- block observation for terminal command output;
- observation: structured surface captures and screenshots when the platform
  permits.

This boundary is intentionally local-only. It is not a network API and does not
yet include remote authentication, capability tokens, or multi-user policy.

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
  macOS WKWebView provider for the current browser path.

ah-store
  SQLite persistence boundary.

ah-control
  JSON-serializable local control protocol.

ah-use
  External harness that validates the product through the control socket.

agenthouse
  GPUI application composition and user-facing shell.
```

## Terminal Strategy

Terminal sessions are the first runtime for local AI tools and operations
commands. They provide process lifecycle, input, resize, output capture, and
structured observation.

The terminal path should remain reliable enough for interactive command-line
agents, but the product direction is not limited to terminals. Terminals are one
execution surface inside a broader operations workspace.

## Browser Strategy

The current browser path uses macOS WKWebView. Browser tabs can navigate, report
URL/title/load state, provide structured surface state, and coexist with
terminal tabs in the same workspace model.

Browser automation is intentionally conservative in `0.1.0`. Actions should be
validated through the product path before being described as supported.

## Attention And Operations Model

AgentHouse is organized around a practical loop:

1. Observe what is happening.
2. Let agents carry bounded work.
3. Preserve evidence as blocks.
4. Escalate only what needs attention.

That model is why the local control plane, blocks, sessions, and surface capture
exist. They are the foundation for future orchestration and automation
operations.
