# AgentHouse-RS

AgentHouse-RS is a native Rust desktop workspace for human + agent collaboration.

This branch is the `0.1.0` Alpha line. It is intentionally narrow: a macOS GPUI app that can open a real workspace folder, run terminal sessions, show native WKWebView browser tabs, split workspace windows, and expose the same model to local agent automation through a JSONL control socket.

## Alpha Scope

Included in `0.1.0`:

- macOS desktop app built with GPUI.
- Folder-backed workspaces with an empty onboarding state.
- Terminal windows and terminal tabs backed by PTY sessions.
- Native browser tabs backed by macOS WKWebView.
- Basic workspace window operations: create, activate, close, tab move, targeted tab open, and split right/down.
- SQLite persistence for workspace/session/control state.
- Local JSONL control socket for agent-driven inspection and smoke tests.
- macOS `.app` bundle generation with `xtask macos-bundle`.

Not included in `0.1.0`:

- Servo browser integration.
- Fingerprint browser behavior.
- FlowBoard.
- Cross-platform desktop support.
- File/folder browser product scope.
- Network-exposed control APIs.

## Requirements

- macOS 13 or newer.
- Rust 1.95 or newer.
- Xcode Command Line Tools for native macOS linking and `iconutil`/`sips` bundle assets.

GPUI and Alacritty are pinned to public Git commits in `Cargo.toml` and `Cargo.lock`. A first clean build needs GitHub access unless those git dependencies are already cached by Cargo.

## Build And Run

```sh
cargo build -p ah-app
cargo run -p ah-app
```

To create a macOS app bundle after building:

```sh
cargo run -p xtask -- macos-bundle --profile debug
```

The bundle is written to `target/debug/AgentHouse.app`. Use `--profile release` after building `ah-app` with `--release`.

## Control Harness

`ah-use` talks to the running app through the local JSONL control socket. Useful checks:

```sh
cargo run -p ah-use -- ping
cargo run -p ah-use -- inspect snapshot
cargo run -p ah-use -- platform-loop
cargo run -p ah-use -- window-api-loop
```

The control socket is local-only and is not an authenticated network API.

## Repository Layout

```text
crates/ah-core            Shared IDs, timestamps, typed foundations
crates/ah-workspace       Workspace, window, tab, and layout model
crates/ah-session         Session metadata and lifecycle state
crates/ah-terminal        PTY and terminal grid boundary
crates/ah-block           Structured work/block model
crates/ah-web             Browser backend trait and lightweight web state
crates/ah-webview-macos   macOS WKWebView backend
crates/ah-store           SQLite persistence boundary
crates/ah-control         JSON-serializable control protocol
crates/ah-use             External control harness and smoke scenarios
crates/ah-app             GPUI application
xtask/                    Project automation
docs/                     Architecture and release notes
```

## License

Apache-2.0. See `LICENSE`.
