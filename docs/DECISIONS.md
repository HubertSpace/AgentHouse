# Architecture Decisions

## ADR-0001: Build A Clean Rust Workspace

Decision: build AgentHouse-RS as a Rust workspace instead of migrating old prototypes directly.

Reason: the product needs native desktop capability, clear crate boundaries, and long-term ownership over terminal, browser, persistence, and control surfaces.

## ADR-0002: macOS Native Browser For 0.1.0

Decision: `0.1.0` uses macOS WKWebView as the browser backend.

Reason: the Alpha needs a real browser path in the product before experimental engines or compatibility research. Browser behavior still flows through `ah-web::BrowserBackend`, but Servo and other engines are outside this release branch.

## ADR-0003: Terminal First, Agent Runtime Second

Decision: start with PTY process sessions and add agent detection as a consumer of terminal/session/block events.

Reason: current external agents already run in terminals. A reliable terminal lifecycle gives AgentHouse a product path before a custom agent runtime exists.

## ADR-0004: Blocks Are The Main Persistence Object

Decision: store work as structured blocks instead of only raw terminal transcript text.

Reason: forwarding, referencing, approvals, browser context, and multi-agent collaboration need stable IDs and typed metadata.

## ADR-0005: Workspaces Contain Windows, Windows Contain Tabs

Decision: the user opens folder-backed workspaces. Each workspace contains AgentHouse windows, and each window contains tabs such as terminal and web.

Reason: workspace, window, tab, and session are different contracts. Keeping them separate allows targeted tab creation, window splitting, and session-level observation without collapsing everything into one navigation object.

## ADR-0006: Sessions Own Blocks And Notification Rings

Decision: blocks and notification rings are scoped to sessions.

Reason: terminal and browser activity should report status locally while still allowing cross-session block forwarding and shared workspace observation.

## ADR-0007: Local JSONL Control Plane For Agents

Decision: expose workspace, window, tab, session, browser, terminal, block, event, and surface operations through a local Unix-domain JSONL socket.

Reason: AgentHouse is a human-agent shared platform. Agents need the same product control plane as humans, but without relying on visual automation for every action. Keeping the boundary local avoids premature network security promises.

## ADR-0008: ah-use Tests The Public Boundary

Decision: validate the app with `ah-use`, a separate crate that talks only through the control socket.

Reason: the important question is whether an external agent can operate the platform through the same boundary future integrations will use. Internal unit tests are necessary, but they do not replace product-path control smoke tests.
