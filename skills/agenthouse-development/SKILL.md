---
name: agenthouse-development
description: Project workflow for AgentHouse Rust/GPUI development, debugging, review, and validation. Use for workspaces, panes, tabs, sessions, terminal, browser, focus, input, persistence, local control APIs, Cargo checks, and product smoke testing.
---

# AgentHouse Development Workflow

Use this workflow for AgentHouse engineering work. It is intentionally practical:
understand the local product contract, make the smallest coherent change, and
verify through the real app path when behavior crosses UI, terminal, browser,
storage, or control boundaries.

## Product Lens

AgentHouse is a local AI operations workspace. It is not primarily a programming
tool. Technical surfaces such as terminals, browsers, sessions, and local
control APIs exist because business and operations work often flows through
them.

Favor changes that make work more observable, recoverable, and controllable for
humans using AI agents.

## Repository Protocol

1. Start from repository facts:
   - check `git status --short --branch`;
   - read relevant crate boundaries in `Cargo.toml`;
   - inspect the modules, call sites, and tests that own the behavior;
   - do not revert unrelated changes.
2. Keep scope tight:
   - follow existing local patterns;
   - avoid broad refactors unless they are required for the requested behavior;
   - add an abstraction only when it removes real complexity or matches an
     established project pattern.
3. Prefer product-path fixes:
   - implement behavior in the real `agenthouse` app path;
   - avoid demo-only or placeholder paths for production behavior;
   - keep temporary diagnostics out of the main UI unless they are intentional
     user-facing controls.
4. Treat ownership, lifetime, and async errors as design feedback. Do not hide
   them with unnecessary clones, global state, or broad locks.

## Product Contracts

- Workspace: a folder-backed working context.
- Window: an AgentHouse work surface inside a workspace.
- Pane: a split area in a window.
- Tab: a switchable surface inside a pane.
- Session: the runtime behind a tab.
- Block: a durable unit of work, evidence, or handoff.

Closing a workspace closes it from the current UI. It must not delete the
workspace folder or persisted workspace contents.

## Risk Levels

- R0: documentation or configuration.
- R1: private local logic or narrow bug fix.
- R2: public API, serialization, errors, or feature flags.
- R3: async, concurrency, lifecycle, cancellation, persistence, or control
  protocol changes.
- R4: FFI, layout, rendering, terminal, browser, focus, input, performance hot
  paths, or macOS integration.
- R5: security boundary, external input, path handling, command execution,
  network exposure, auth, secrets, deserialization, or supply-chain changes.

For R3 and above, focused tests are not enough when the behavior is visible in
the app. Run the real app path or local control smoke where feasible.

## Rust Rules

- Keep library errors typed; avoid exposing `anyhow`, `String`, or boxed errors
  from shared library boundaries.
- Do not weaken tests, validation, authorization, secret handling, or resource
  limits to make code compile.
- Before adding dependencies, check features, license, maintenance, transitive
  dependencies, and release impact.
- Avoid unsafe code. If unavoidable, document the boundary and add focused
  tests.
- Do not claim performance improvements without measurements. If there is no
  measurement, describe the structural change only.

## UI, Terminal, And Browser Rules

- Maintain focus ownership per tab/session.
- Terminal output should be event-driven. Avoid fixed polling as the primary
  delivery mechanism.
- Terminal rendering should use a paint-oriented path rather than building one
  UI child per terminal cell.
- Terminal input should support text input, paste, semantic keys, IME, resize,
  interrupt, restart, and shutdown behavior.
- Browser tabs should define navigation, URL state, focus, loading/error state,
  close behavior, and coexistence with terminal tabs.
- The current browser product path is native macOS WKWebView.
- Pane splitting should behave predictably: split the current area right/down,
  then preserve table-like resizing behavior.

## Verification

Default checks:

```sh
cargo fmt --all
cargo check -p agenthouse --all-targets
cargo test -p agenthouse
scripts/check-release-licenses.sh
```

For broader shared changes:

```sh
cargo check --workspace --all-targets
cargo test --workspace
```

For app-path validation:

```sh
cargo build -p agenthouse -p ah-use
cargo run -p xtask -- macos-bundle --profile debug
open target/debug/AgentHouse.app
target/debug/ah-use ping
target/debug/ah-use inspect snapshot
```

Use focused `ah-use` scenarios when touching control APIs, terminal behavior,
browser behavior, window/tab operations, lifecycle, or persistence.

## Release Hygiene

- Keep local logs, databases, screenshots, private prompts, machine paths, and
  task-continuation notes out of commits.
- Run `scripts/check-release-licenses.sh` before public release changes.
- Keep `THIRD_PARTY_NOTICES.md` current when dependencies or bundled assets
  change.
- Public docs should explain product behavior and dependency facts. Internal
  investigation notes belong outside the public release tree.

## Reporting Back

When finishing a change, summarize:

- behavior changed;
- files or modules touched;
- checks run;
- what was not manually validated.
