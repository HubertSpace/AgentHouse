# Roadmap

## 0.1.0 Alpha

The Alpha target is a macOS-only product slice suitable for external review and early open-source iteration.

Must be true before tagging:

- Empty state opens without sidebar noise and offers one clear folder picker.
- Opening a folder creates a workspace rooted at that folder and starts a terminal pane there.
- Closing the last workspace returns to the empty state without panics.
- Terminal tabs support command execution, semantic keys, paste, resize, restart, interrupt, and structured screen capture.
- Native WKWebView tabs open through the product path and report URL/session/surface state through control APIs.
- Window/tab control supports targeted terminal tabs, targeted web tabs, close/move/activate, and split right/down.
- macOS bundle generation produces `AgentHouse.app` with the Alpha icon.
- `cargo fmt`, workspace check/test, focused app/control tests, and GPUI/control smoke are documented for the release.

Explicitly out of scope for `0.1.0`:

- Servo.
- Fingerprint browser behavior.
- FlowBoard.
- Cross-platform ports.
- File/folder browser product scope.
- Remote/network control APIs.

## 0.1.x Hardening

- Reduce remaining compiler warnings, especially in macOS WebView FFI code.
- Tighten true embedded WKWebView interaction validation beyond snapshot/frame observation.
- Add more deterministic UI smoke coverage for focus, IME, split resizing, and multi-window terminal routing.
- Improve error surfaces for WebView initialization and navigation failures.
- Add release packaging notes and signed/notarized distribution steps when needed.

## Later Milestones

- Notification-ring UX beyond the current session state indicator.
- Agent detection layer over terminal/session/block events.
- Rich block canvas and richer cross-session workflow affordances.
- Optional browser-engine experiments on branches separate from the Alpha release line.
- Cross-platform implementation once macOS product semantics are stable.
