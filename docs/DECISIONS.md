# Architecture Decisions

## ADR-0001: Build A Native Rust Workspace

Decision: build AgentHouse as a Rust workspace with clear crate boundaries.

Reason: the product needs native desktop capability, local persistence, terminal
control, browser embedding, and a stable local control boundary for agents.

## ADR-0002: Start Local-First

Decision: `0.1.0` exposes only a local Unix-domain JSONL control socket.

Reason: AgentHouse can run commands and expose workspace state. Remote control
needs authentication, capability policy, audit controls, and user approval
semantics before it should exist.

## ADR-0003: Terminal Sessions First

Decision: start with PTY process sessions, then add richer agent orchestration
on top of observable session events and blocks.

Reason: many existing AI agents and business automation tools already run as
local command-line processes. A reliable terminal lifecycle gives AgentHouse a
useful product path before a custom agent runtime exists.

## ADR-0004: Blocks Are The Durable Work Object

Decision: store work as structured blocks instead of only raw terminal
transcripts.

Reason: forwarding, referencing, approvals, browser context, audit trails, and
multi-agent collaboration need stable IDs and typed metadata.

## ADR-0005: Workspaces Contain Windows, Windows Contain Panes And Tabs

Decision: the user opens folder-backed workspaces. Each workspace contains
AgentHouse windows; windows can split into panes; panes contain tabs such as
terminal and web.

Reason: workspace, window, pane, tab, and session are different contracts.
Keeping them separate allows predictable splitting, targeted tab creation,
session-level observation, and future operations dashboards.

## ADR-0006: Human Approval Remains A Product Boundary

Decision: AgentHouse should make agent work visible and controllable before it
automates irreversible external actions.

Reason: the product goal is attention sovereignty, not hidden autonomy. Humans
should spend less attention on routine loops while keeping approval, judgment,
and accountability over meaningful actions.

## ADR-0007: ah-use Tests The Public Control Boundary

Decision: validate the app with `ah-use`, a separate crate that talks through
the local control socket.

Reason: the important question is whether an external agent can operate the
platform through the same boundary future integrations will use. Internal unit
tests are necessary, but they do not replace product-path control smoke tests.
