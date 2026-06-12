# Alpha Release Notes

## 0.1.0 Release Boundary

This branch is the macOS Alpha release line. Keep it focused on:

- folder-backed workspace creation;
- terminal windows and terminal tabs;
- native WKWebView browser tabs;
- basic window/tab/split operations;
- local JSONL control and `ah-use` product smoke tests;
- macOS app bundle generation.

Do not reintroduce Servo, fingerprint browser behavior, FlowBoard, cross-platform code, file/folder browser scope, or tracked local browser/cache data into this branch.

## Open-Source Readiness

- GPUI and Alacritty are pinned to public Git commits in `Cargo.toml` and `Cargo.lock`. Clean public builds need GitHub access for those dependencies unless the Cargo git cache is already populated.
- The macOS WKWebView backend currently checks cleanly without deprecated `block2::ConcreteBlock` or Rust 2024 `static mut` warnings.
- The generated `.app` bundle is a local development bundle. Signing, notarization, release DMG packaging, and updater behavior are not part of `0.1.0` Alpha.
- Some private terminal command-capture and notification-ring UI helpers remain in `ah-app` for continuity and test coverage, but they are not exposed as `0.1.0` product features.
- The current browser path validates WKWebView navigation/frame capture through AgentHouse, but full interactive embedded NSView behavior still needs stricter product-path validation before claiming complete browser embedding.

## Required Verification

- `cargo fmt --all`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo run -p xtask -- macos-bundle --profile debug`
- GPUI/control smoke: empty state, create workspace from folder/control, targeted terminal tab, targeted web tab, split right/down, capture surface, close app process.
