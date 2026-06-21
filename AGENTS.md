# AgentHouse Agent Instructions

This repository includes a project-specific development workflow at:

```text
skills/agenthouse-development/SKILL.md
```

Use it for AgentHouse Rust/GPUI development, debugging, review, and validation,
especially when changing workspaces, panes, tabs, sessions, terminal behavior,
browser behavior, focus, input, layout, persistence, or local control APIs.

## Working Rules

- Start from repository facts: check `git status --short --branch`, read the
  relevant crate and module boundaries, and inspect call sites before editing.
- Do not revert unrelated user changes.
- Prefer existing local patterns over new abstractions.
- Keep changes scoped to the requested product behavior.
- Use `rg` when available; otherwise use `git grep` or narrowly scoped shell
  searches.
- Use `apply_patch` for manual file edits.
- For UI, terminal, browser, input, focus, layout, persistence, or lifecycle
  changes, Cargo checks are necessary but not sufficient; validate the real app
  path when feasible.

## Common Checks

```sh
cargo fmt --all
CARGO_NET_OFFLINE=true cargo check -p agenthouse --all-targets
CARGO_NET_OFFLINE=true cargo test -p agenthouse
scripts/check-release-licenses.sh
```

For macOS bundle validation:

```sh
CARGO_NET_OFFLINE=true cargo build -p agenthouse -p ah-use
cargo run -p xtask -- macos-bundle --profile debug
open target/debug/AgentHouse.app
target/debug/ah-use ping
target/debug/ah-use inspect snapshot
```

## Product Contracts

- Workspace: a user working context backed by a local folder.
- Window: an AgentHouse work surface inside a workspace.
- Pane: a split area inside a window.
- Tab: a switchable surface inside a pane, such as terminal or browser.
- Session: the runtime context behind a tab.
- Block: a durable unit of work or evidence produced by a session.
- Closing a workspace removes it from the current UI; it must not delete the
  underlying folder or persisted workspace contents.
- The current browser path is the native macOS WKWebView path.
