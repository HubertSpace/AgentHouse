# Beta Release Notes

## 0.1.0 Release Boundary

AgentHouse `0.1.0` is the first public Beta line. It is intentionally narrow:

- folder-backed workspace creation;
- terminal windows and terminal tabs;
- native WKWebView browser tabs on macOS;
- basic window, pane, tab, and split operations;
- local JSONL control and `ah-use` product smoke tests;
- macOS app bundle generation.

The release is local-first. It does not include remote control APIs, production
signing/notarization, cross-platform desktop support, a full file manager, or
browser identity tooling.

## Open-Source Readiness

- Source license: Apache-2.0 (`LICENSE`).
- Third-party dependencies and bundled assets are recorded in
  `THIRD_PARTY_NOTICES.md`.
- Before release, `scripts/check-release-licenses.sh` must pass.
- The generated `.app` bundle is a local development bundle. Signing,
  notarization, release DMG packaging, and updater behavior are not part of
  `0.1.0` Beta.
- The current browser path validates WKWebView navigation and surface state
  through AgentHouse. Browser actions should be tested through the product path
  before being described as supported.

## Required Verification

- `cargo fmt --all`
- `cargo check -p agenthouse --all-targets`
- `cargo test -p agenthouse`
- `scripts/check-release-licenses.sh`
- `cargo build -p agenthouse -p ah-use`
- `cargo run -p xtask -- macos-bundle --profile debug`
- GPUI/control smoke: empty state, create workspace, open terminal tab, open web
  tab, split right/down, capture surface, close app process.
