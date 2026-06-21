# Contributing

AgentHouse `0.1.0` is a Beta macOS Rust/GPUI workspace. Keep changes
focused on the release boundary described in `README.md` and
`docs/BETA_RELEASE.md`.

## Workflow

- Read `AGENTS.md` before making code changes.
- Use `skills/agenthouse-development/SKILL.md` for Rust/GPUI
  development, debugging, review, and product-path validation.
- Keep work scoped to the requested behavior and avoid unrelated refactors.
- Do not commit local state, logs, database files, build artifacts, editor
  settings, private prompts, or machine-specific paths.

## Checks

Run focused checks for the area changed. For broad changes, use:

```sh
cargo fmt --all
CARGO_NET_OFFLINE=true cargo check --workspace --all-targets
CARGO_NET_OFFLINE=true cargo test --workspace
```

For UI, terminal, browser, focus, input, layout, or lifecycle changes, also run
the real app path and a control smoke test:

```sh
CARGO_NET_OFFLINE=true cargo build -p agenthouse -p ah-use
cargo run -p xtask -- macos-bundle --profile debug
open target/debug/AgentHouse.app
target/debug/ah-use ping
target/debug/ah-use inspect snapshot
```

## Licensing

Unless explicitly stated otherwise, contributions to AgentHouse are submitted
under the repository's Apache-2.0 license.

Before adding or changing dependencies, check the new dependency's license,
feature set, maintenance status, and transitive dependency tree. Update
`THIRD_PARTY_NOTICES.md` when dependency or bundled asset facts change.
