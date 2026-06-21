# Roadmap

AgentHouse is moving toward a local AI operations center for business work.

The product thesis is simple: AI should not compete for a person's attention.
It should help defend it. AgentHouse starts by making local agent work visible
and controllable, then grows toward orchestration, repeatable operations, and
attention-aware workflows.

## 0.1.0 Beta: Local Workbench

The Beta is a macOS-only product slice suitable for external review and early
open-source iteration.

Must be true before tagging:

- Empty state opens cleanly and offers one clear folder picker.
- Opening a folder creates a workspace rooted at that folder and starts a
  terminal pane there.
- Closing the last workspace returns to the empty state without panics.
- Terminal tabs support command execution, semantic keys, paste, resize,
  restart, interrupt, and structured screen capture.
- Native WKWebView tabs open through the product path and report URL, session,
  and surface state through control APIs.
- Window and tab control supports targeted terminal tabs, targeted web tabs,
  close, move, activate, and split right/down.
- macOS bundle generation produces `AgentHouse.app` with the Beta icon.
- Formatting, focused Rust checks, focused tests, release license checks, and
  GPUI/control smoke testing are documented for the release.

Out of scope for `0.1.0`:

- Cross-platform desktop support.
- Remote or network-exposed control APIs.
- Production signing, notarization, DMG packaging, and auto-update.
- Full file manager scope.
- Browser identity or fingerprint tooling.

## 0.1.x: Reliability And Fit

- Tighten workspace close/reopen behavior and persisted workspace recovery.
- Improve terminal selection, focus, IME, resize, and split-pane behavior.
- Improve native browser sizing, navigation error surfaces, and interaction
  validation.
- Add more deterministic smoke coverage for focus, split resizing, and
  multi-session routing.
- Reduce rough edges in settings, theme, language, and onboarding.
- Add release packaging notes when distribution moves beyond local builds.

## 0.2: Agent Orchestration

Move from "one agent in one terminal" to visible coordination:

- role-based agent sessions;
- block forwarding and handoff queues;
- approval requests and human decision checkpoints;
- session status, progress, and exception tracking;
- shared context packs for a workspace;
- repeatable task templates for operations work.

The intended user is not only a programmer. It is an operator, founder,
marketer, analyst, researcher, or support lead who needs AI to carry work
without hiding what changed.

## 0.3: Automation Operations Center

Turn repeated business routines into controlled operations:

- scheduled checks and recurring workflows;
- content and channel monitoring;
- customer follow-up queues;
- report generation and review loops;
- exception triage;
- audit trails for what an agent did, saw, and handed back;
- human approval before irreversible external actions.

This is where AgentHouse becomes less like a terminal wrapper and more like a
local operations room.

## 0.4+: Attention Layer

The long-term product layer is attention-aware:

- show what changed, not everything that happened;
- separate urgent, important, waiting, and safe-to-ignore work;
- let users delegate loops without losing context;
- preserve decisions and evidence;
- make escalation explicit instead of noisy;
- help humans spend attention where it compounds.

AgentHouse should make AI feel less like another inbox and more like a quiet
system that keeps work moving.
