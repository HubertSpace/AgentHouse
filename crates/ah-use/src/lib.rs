use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use ah_control::{
    BlockSummary, ControlCommand, ControlEvent, ControlRequest, ControlResponse, ControlResult,
    ControlStreamMessage, SessionSummary, SurfaceCapture, TerminalKeyInput,
    TerminalKeyInputModifiers, UiLanguagePreference, UiThemeModePreference,
    UiThemeSchemePreference, WindowSplitDirection, WindowSummary, WindowTabSummary,
    WorkspaceSummary,
};
use ah_core::{BlockId, SessionId, TabId, WindowId, WorkspaceId};
use ah_terminal::paste_sequence_for_text;
use anyhow::{Context, Result, bail};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(45);
const DEBATE_TURN_TIMEOUT: Duration = Duration::from_secs(240);

fn default_workspace_root() -> PathBuf {
    std::env::var_os("AGENTHOUSE_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn run_cli_from_env() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    run_cli(&args)
}

pub fn run_cli(args: &[String]) -> Result<()> {
    let scenario = args.get(1).map(String::as_str).unwrap_or("inspect");
    let client = ControlClient::default();

    match scenario {
        "ping" => {
            let response = client.request(ControlCommand::Ping)?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            Ok(())
        }
        "inspect" => run_inspect(&client, &args[2..]),
        "act" => run_act(&client, &args[2..]),
        "wait" => run_wait(&client, &args[2..]),
        "watch" => run_watch(&client, &args[2..]),
        "claude-smoke" => run_claude_smoke(&client),
        "claude-tui-smoke" => run_claude_tui_smoke(&client),
        "claude-tui-key-smoke" => run_claude_tui_key_smoke(&client),
        "claude-tui-prompt-smoke" => run_claude_tui_prompt_smoke(&client),
        "multi-window" => run_multi_window(&client),
        "platform-loop" => run_platform_loop(&client),
        "window-api-loop" => run_window_api_loop(&client),
        "session-lifecycle" => run_session_lifecycle(&client),
        "interactive-stdin" => run_interactive_stdin(&client),
        "terminal-grid-loop" => run_terminal_grid_loop(&client),
        "terminal-key-loop" => run_terminal_key_loop(&client),
        "terminal-paste-loop" => run_terminal_paste_loop(&client),
        "terminal-tui-loop" => run_terminal_tui_loop(&client),
        "watch-loop" => run_watch_loop(&client),
        "persist-write" => run_persist_write(&client),
        "persist-read" => run_persist_read(&client),
        "handoff-loop" => run_handoff_loop(&client),
        "debate-loop" => run_debate_loop(&client),
        "stress-loop" => run_stress_loop(&client, &args[2..]),
        other => bail!("unknown ah-use scenario: {other}"),
    }
}

fn run_inspect(client: &ControlClient, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str).unwrap_or("snapshot");
    let response = match target {
        "snapshot" => client.request(ControlCommand::Snapshot)?,
        "settings" => client.request(ControlCommand::GetAppSettings)?,
        "workspaces" => client.request(ControlCommand::ListWorkspaces)?,
        "windows" => client.request(ControlCommand::ListWindows {
            workspace_id: parse_optional_workspace_id(args.get(1))?,
        })?,
        "tabs" => client.request(ControlCommand::ListWindowTabs {
            window_id: parse_required_window_id(args.get(1), "window id")?,
        })?,
        "sessions" => client.request(ControlCommand::ListSessions {
            workspace_id: parse_optional_workspace_id(args.get(1))?,
        })?,
        "session" => client.request(ControlCommand::GetSession {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "blocks" => client.request(ControlCommand::ListBlocks {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "surface" => client.request(ControlCommand::CaptureSurface {
            window_id: parse_optional_window_id(args.get(1))?,
        })?,
        "session-surface" => client.request(ControlCommand::CaptureSessionSurface {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "events" => client.request(ControlCommand::ListEvents {
            since_sequence: parse_optional_u64(args.get(1), "since sequence")?,
            limit: parse_optional_usize(args.get(2), "limit")?,
        })?,
        other => bail!("unknown inspect target: {other}"),
    };
    print_json(&response)
}

fn run_act(client: &ControlClient, args: &[String]) -> Result<()> {
    let action = args.first().map(String::as_str).unwrap_or("");
    let response = match action {
        "create-workspace" => client.request(ControlCommand::CreateWorkspace {
            name: required_arg(args.get(1), "workspace name")?.to_string(),
            root: args.get(2).map(PathBuf::from),
        })?,
        "activate-workspace" => client.request(ControlCommand::ActivateWorkspace {
            workspace_id: parse_required_workspace_id(args.get(1), "workspace id")?,
        })?,
        "create-window" => client.request(ControlCommand::CreateWindow {
            workspace_id: parse_required_workspace_id(args.get(1), "workspace id")?,
            title: required_arg(args.get(2), "window title")?.to_string(),
        })?,
        "activate-window" => client.request(ControlCommand::ActivateWindow {
            workspace_id: parse_required_workspace_id(args.get(1), "workspace id")?,
            window_id: parse_required_window_id(args.get(2), "window id")?,
        })?,
        "activate-tab" => client.request(ControlCommand::ActivateWindowTab {
            window_id: parse_required_window_id(args.get(1), "window id")?,
            tab_id: parse_required_tab_id(args.get(2), "tab id")?,
        })?,
        "open-terminal" => client.request(ControlCommand::OpenTerminalWindow {
            workspace_id: parse_required_workspace_id(args.get(1), "workspace id")?,
            title: required_arg(args.get(2), "terminal title")?.to_string(),
            cwd: args.get(3).map(PathBuf::from),
        })?,
        "open-terminal-tab" => client.request(ControlCommand::OpenTerminalTab {
            window_id: parse_required_window_id(args.get(1), "window id")?,
            title: required_arg(args.get(2), "terminal title")?.to_string(),
            cwd: args.get(3).map(PathBuf::from),
        })?,
        "open-web" => client.request(ControlCommand::OpenWebWindow {
            workspace_id: parse_required_workspace_id(args.get(1), "workspace id")?,
            title: required_arg(args.get(2), "web title")?.to_string(),
            url: required_arg(args.get(3), "url")?.to_string(),
        })?,
        "open-web-tab" => client.request(ControlCommand::OpenWebTab {
            window_id: parse_required_window_id(args.get(1), "window id")?,
            title: required_arg(args.get(2), "web title")?.to_string(),
            url: required_arg(args.get(3), "url")?.to_string(),
        })?,
        "split-window" => client.request(ControlCommand::SplitWindow {
            window_id: parse_required_window_id(args.get(1), "window id")?,
            direction: parse_window_split_direction(args.get(2))?,
        })?,
        "set-language" => client.request(ControlCommand::SetUiLanguage {
            language: parse_ui_language_preference(args.get(1))?,
        })?,
        "set-theme" | "set-color" => client.request(ControlCommand::SetUiThemeScheme {
            scheme: parse_ui_theme_scheme_preference(args.get(1))?,
        })?,
        "set-theme-mode" | "set-mode" => client.request(ControlCommand::SetUiThemeMode {
            mode: parse_ui_theme_mode_preference(args.get(1))?,
        })?,
        "run" => client.request(ControlCommand::RunTerminalCommand {
            session_id: parse_required_session_id(args.get(1), "session id")?,
            command: required_arg(args.get(2), "command")?.to_string(),
        })?,
        "write" => client.request(ControlCommand::WriteTerminalInput {
            session_id: parse_required_session_id(args.get(1), "session id")?,
            input: required_arg(args.get(2), "input")?.to_string(),
        })?,
        "key" => client.request(ControlCommand::SendTerminalKey {
            session_id: parse_required_session_id(args.get(1), "session id")?,
            key: parse_terminal_key_args(args.get(2), args.get(3))?,
        })?,
        "interrupt" => client.request(ControlCommand::InterruptSession {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "terminate" => client.request(ControlCommand::TerminateSession {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "restart" => client.request(ControlCommand::RestartSession {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "resize" => client.request(ControlCommand::ResizeTerminal {
            session_id: parse_required_session_id(args.get(1), "session id")?,
            cols: parse_required_u16(args.get(2), "cols")?,
            rows: parse_required_u16(args.get(3), "rows")?,
        })?,
        "ack-ring" => client.request(ControlCommand::AckSessionRing {
            session_id: parse_required_session_id(args.get(1), "session id")?,
        })?,
        "forward-block" => client.request(ControlCommand::ForwardBlock {
            source_session_id: parse_required_session_id(args.get(1), "source session id")?,
            block_id: parse_required_block_id(args.get(2), "block id")?,
            target_session_id: parse_required_session_id(args.get(3), "target session id")?,
        })?,
        other => bail!("unknown act action: {other}"),
    };
    print_json(&response)
}

fn run_wait(client: &ControlClient, args: &[String]) -> Result<()> {
    let condition = args.first().map(String::as_str).unwrap_or("");
    match condition {
        "block" => {
            let session_id = parse_required_session_id(args.get(1), "session id")?;
            let marker = required_arg(args.get(2), "marker")?;
            let timeout = parse_optional_duration_secs(args.get(3), DEFAULT_TIMEOUT)?;
            let blocks = wait_for_block_with_timeout(client, session_id, marker, timeout)?;
            print_json(&serde_json::json!({
                "type": "wait_result",
                "condition": "block",
                "session_id": session_id,
                "marker": marker,
                "blocks": blocks,
            }))
        }
        "ring" => {
            let session_id = parse_required_session_id(args.get(1), "session id")?;
            let state = required_arg(args.get(2), "ring state")?;
            let timeout = parse_optional_duration_secs(args.get(3), DEFAULT_TIMEOUT)?;
            let session = wait_for_ring_state(client, session_id, state, timeout)?;
            print_json(&serde_json::json!({
                "type": "wait_result",
                "condition": "ring",
                "session": session,
            }))
        }
        "terminal-tail" => {
            let session_id = parse_required_session_id(args.get(1), "session id")?;
            let marker = required_arg(args.get(2), "marker")?;
            let timeout = parse_optional_duration_secs(args.get(3), DEFAULT_TIMEOUT)?;
            let surface = wait_for_terminal_tail(client, session_id, marker, timeout)?;
            print_json(&serde_json::json!({
                "type": "wait_result",
                "condition": "terminal_tail",
                "surface": surface,
            }))
        }
        "event" => {
            let topic = required_arg(args.get(1), "topic")?;
            let since_sequence = parse_optional_u64(args.get(2), "since sequence")?;
            let timeout = parse_optional_duration_secs(args.get(3), DEFAULT_TIMEOUT)?;
            let event = wait_for_event_topic(client, topic, since_sequence, timeout)?;
            print_json(&serde_json::json!({
                "type": "wait_result",
                "condition": "event",
                "event": event,
            }))
        }
        other => bail!("unknown wait condition: {other}"),
    }
}

fn run_watch(client: &ControlClient, args: &[String]) -> Result<()> {
    let max_messages = parse_optional_usize(args.first(), "max messages")?.unwrap_or(10);
    let since_sequence = parse_optional_u64(args.get(1), "since sequence")?;
    client.watch_events(since_sequence, max_messages)
}

fn run_claude_smoke(client: &ControlClient) -> Result<()> {
    let snapshot = expect_snapshot(client.request(ControlCommand::Snapshot)?)?;
    let workspace_id = active_workspace_id(&snapshot)?;
    let session_id = first_terminal_session(&snapshot.windows)?;

    client.request(ControlCommand::CaptureSurface { window_id: None })?;
    let marker = "AH_CLAUDE_SMOKE_SINGLE_20260603";
    run_claude_command(client, session_id, marker)?;
    let blocks = wait_for_claude_block(client, session_id, marker)?;
    let events = client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(50),
    })?;

    println!("workspace={workspace_id:?}");
    println!("session={session_id:?}");
    println!("matched_blocks={}", blocks.len());
    println!("{}", serde_json::to_string_pretty(&events)?);
    Ok(())
}

fn run_claude_tui_smoke(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use Claude TUI",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Claude TUI terminal")?;
    let session_id = terminal
        .session_id
        .context("Claude TUI terminal should have a session")?;

    write_terminal_input(client, session_id, "claude\n".to_string())?;
    let surface = wait_for_terminal_snapshot(
        client,
        session_id,
        terminal_snapshot_looks_like_claude_tui,
        Duration::from_secs(60),
        "Claude Code TUI screen",
    )?;
    let lines = terminal_snapshot_trimmed_lines(&surface);

    let _ = write_terminal_input(client, session_id, "\x03".to_string());
    thread::sleep(Duration::from_millis(250));
    let _ = write_terminal_input(client, session_id, "\x1b".to_string());
    thread::sleep(Duration::from_millis(250));
    let _ = write_terminal_input(client, session_id, "q".to_string());
    thread::sleep(Duration::from_millis(250));
    let _ = interrupt_session(client, session_id);

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!(
        "claude_tui_observed=true alt_screen={}",
        surface
            .get("alt_screen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    );
    println!("screen_lines={}", lines.join(" | "));
    Ok(())
}

fn run_claude_tui_key_smoke(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use Claude TUI key",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Claude TUI key terminal")?;
    let session_id = terminal
        .session_id
        .context("Claude TUI key terminal should have a session")?;

    write_terminal_input(client, session_id, "claude\n".to_string())?;
    let (after_key, trust_prompt_seen) = wait_for_claude_tui_ready(client, session_id)?;
    let lines = terminal_snapshot_trimmed_lines(&after_key);

    cleanup_claude_tui_session(client, session_id);

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("claude_tui_key_observed=true trust_prompt_seen={trust_prompt_seen}");
    println!("screen_lines={}", lines.join(" | "));
    Ok(())
}

fn run_claude_tui_prompt_smoke(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use Claude TUI prompt",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Claude TUI prompt terminal")?;
    let session_id = terminal
        .session_id
        .context("Claude TUI prompt terminal should have a session")?;

    let result = (|| -> Result<(bool, Vec<String>, Vec<String>)> {
        write_terminal_input(client, session_id, "claude\n".to_string())?;
        let (ready, trust_prompt_seen) = wait_for_claude_tui_ready(client, session_id)?;
        let ready_lines = terminal_snapshot_trimmed_lines(&ready);

        let marker = "AH_CLAUDE_TUI_PROMPT_20260603";
        let prompt = claude_tui_marker_prompt(marker);
        let paste = paste_sequence_for_text(&prompt).context("Claude TUI prompt should paste")?;
        write_terminal_input(client, session_id, paste)?;
        send_terminal_key(client, session_id, TerminalKeyInput::new("enter"))?;

        let answered = wait_for_terminal_snapshot(
            client,
            session_id,
            |snapshot| terminal_snapshot_text(snapshot).contains(marker),
            DEBATE_TURN_TIMEOUT,
            "Claude Code TUI response marker",
        )?;
        let answer_lines = terminal_snapshot_trimmed_lines(&answered);

        Ok((trust_prompt_seen, ready_lines, answer_lines))
    })();

    cleanup_claude_tui_session(client, session_id);

    let (trust_prompt_seen, ready_lines, answer_lines) = result?;
    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("claude_tui_prompt_observed=true trust_prompt_seen={trust_prompt_seen}");
    println!("ready_screen_lines={}", ready_lines.join(" | "));
    println!("answer_screen_lines={}", answer_lines.join(" | "));
    Ok(())
}

fn wait_for_claude_tui_ready(
    client: &ControlClient,
    session_id: SessionId,
) -> Result<(serde_json::Value, bool)> {
    let initial = wait_for_terminal_snapshot(
        client,
        session_id,
        terminal_snapshot_looks_like_claude_tui,
        Duration::from_secs(60),
        "Claude Code TUI screen before key operation",
    )?;

    if terminal_snapshot_looks_like_claude_trust_prompt(&initial) {
        send_terminal_key(client, session_id, TerminalKeyInput::new("enter"))?;
        let after_trust = wait_for_terminal_snapshot(
            client,
            session_id,
            terminal_snapshot_looks_like_claude_after_trust,
            Duration::from_secs(60),
            "Claude Code TUI after trust confirmation",
        )?;
        Ok((after_trust, true))
    } else {
        Ok((initial, false))
    }
}

fn claude_tui_marker_prompt(marker: &str) -> String {
    let Some((prefix, suffix)) = marker.rsplit_once('_') else {
        return format!("Respond with exactly this token and no other text: {marker}");
    };
    format!(
        "Respond with exactly the token formed by joining `{prefix}` and `{suffix}` with one underscore. Do not include any other text."
    )
}

fn cleanup_claude_tui_session(client: &ControlClient, session_id: SessionId) {
    let _ = send_terminal_key(
        client,
        session_id,
        TerminalKeyInput {
            key: "c".to_string(),
            text: None,
            modifiers: TerminalKeyInputModifiers {
                control: true,
                ..TerminalKeyInputModifiers::default()
            },
        },
    );
    thread::sleep(Duration::from_millis(250));
    let _ = send_terminal_key(client, session_id, TerminalKeyInput::new("escape"));
    thread::sleep(Duration::from_millis(250));
    let _ = send_terminal_key(client, session_id, TerminalKeyInput::text("q", "q"));
    thread::sleep(Duration::from_millis(250));
    let _ = interrupt_session(client, session_id);
}

fn run_multi_window(client: &ControlClient) -> Result<()> {
    let snapshot = expect_snapshot(client.request(ControlCommand::Snapshot)?)?;
    let workspace_id = active_workspace_id(&snapshot)?;

    let first = open_terminal(client, workspace_id, "Claude smoke A")?;
    let second = open_terminal(client, workspace_id, "Claude smoke B")?;

    let first_session = first
        .session_id
        .context("first terminal window should have a session")?;
    let second_session = second
        .session_id
        .context("second terminal window should have a session")?;

    run_claude_command(client, first_session, "AgentHouse-use window A")?;
    run_claude_command(client, second_session, "AgentHouse-use window B")?;

    let first_blocks = wait_for_claude_block(client, first_session, "AgentHouse-use window A")?;
    let second_blocks = wait_for_claude_block(client, second_session, "AgentHouse-use window B")?;
    let surface = client.request(ControlCommand::CaptureSurface {
        window_id: Some(second.id),
    })?;

    println!("workspace={workspace_id:?}");
    println!(
        "first_session={first_session:?} blocks={}",
        first_blocks.len()
    );
    println!(
        "second_session={second_session:?} blocks={}",
        second_blocks.len()
    );
    println!("{}", serde_json::to_string_pretty(&surface)?);
    Ok(())
}

fn run_platform_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use platform loop",
        Some(default_workspace_root()),
    )?;

    let initial_windows = list_windows(client, Some(workspace.id))?;
    let root_window = initial_windows
        .iter()
        .find(|window| window.content_type == "terminal")
        .cloned()
        .context("create_workspace should open an initial terminal window")?;

    let terminal_tab = open_terminal_tab(
        client,
        root_window.id,
        "Agent control terminal",
        Some(default_workspace_root()),
    )?;
    let web_url = "https://example.com/";
    let web_tab = open_web_tab(client, root_window.id, "Agent docs browser", web_url)?;

    let tabs = list_window_tabs(client, root_window.id)?;
    require_tab(&tabs, terminal_tab.id, "terminal")?;
    require_tab(&tabs, web_tab.id, "web")?;

    let terminal_session = terminal_tab
        .session_id
        .context("platform terminal window should have a session")?;
    activate_window_tab(client, root_window.id, terminal_tab.id)?;
    let marker = "AgentHouse-platform-loop";
    run_shell_command(
        client,
        terminal_session,
        format!("printf '%s\\n' {}", shell_quote(marker)),
    )?;
    let terminal_blocks = wait_for_block(client, terminal_session, marker)?;
    let before_ack = get_session(client, terminal_session)?;
    if before_ack.unread_count == 0 {
        bail!("terminal session ring should be unread after command completion");
    }
    let after_ack = ack_session_ring(client, terminal_session)?;
    if after_ack.unread_count != 0 {
        bail!("terminal session ring ack should clear unread count");
    }

    activate_window_tab(client, root_window.id, web_tab.id)?;
    let web_surface = wait_for_web_preview(client, root_window.id, web_url, DEFAULT_TIMEOUT)?;
    require_surface_snapshot(&web_surface)?;

    activate_window_tab(client, root_window.id, terminal_tab.id)?;
    let terminal_surface = expect_surface(client.request(ControlCommand::CaptureSurface {
        window_id: Some(root_window.id),
    })?)?;
    expect_surface_content(&terminal_surface, "terminal")?;
    require_surface_snapshot(&terminal_surface)?;
    if terminal_surface.session.as_ref().map(|session| session.id) != Some(terminal_session) {
        bail!("terminal surface should expose the active terminal session");
    }

    let sessions = list_sessions(client, workspace.id)?;
    if !sessions
        .iter()
        .any(|session| session.id == terminal_session)
    {
        bail!("list_sessions should include the platform terminal session");
    }
    let split_window = split_window(client, root_window.id, WindowSplitDirection::Right)?;
    let windows_after_split = list_windows(client, Some(workspace.id))?;
    if windows_after_split.len() <= initial_windows.len() {
        bail!("split_window should increase window count");
    }
    if split_window.content_type != "terminal" {
        bail!(
            "split_window should create a terminal pane, got {}",
            split_window.content_type
        );
    }
    let events = expect_events(client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(100),
    })?)?;
    require_event_topic(&events, "workspace")?;
    require_event_topic(&events, "window")?;
    require_event_topic(&events, "session")?;

    println!(
        "workspace={:?} window={:?} tabs={} windows_after_split={}",
        workspace.id,
        root_window.id,
        tabs.len(),
        windows_after_split.len()
    );
    println!(
        "terminal_session={terminal_session:?} blocks={} unread_before_ack={} unread_after_ack={}",
        terminal_blocks.len(),
        before_ack.unread_count,
        after_ack.unread_count
    );
    println!("{}", serde_json::to_string_pretty(&terminal_surface)?);
    Ok(())
}

fn run_window_api_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use window api",
        Some(default_workspace_root()),
    )?;
    let source = open_terminal(client, workspace.id, "Window API source terminal")?;
    let source_window_id = source.id;
    let source_tab_id = source
        .active_tab_id
        .context("source terminal should return active tab")?;
    let target = create_window(client, workspace.id, "Second Window")?;
    set_workspace_layout(client, workspace.id, ah_workspace::LayoutMode::Columns)?;
    move_window_tab(client, source_window_id, source_tab_id, target.id)?;

    let target_tabs = list_window_tabs(client, target.id)?;
    require_tab(&target_tabs, source_tab_id, "terminal")?;
    activate_window_tab(client, target.id, source_tab_id)?;
    let terminal_surface = expect_surface(client.request(ControlCommand::CaptureSurface {
        window_id: Some(target.id),
    })?)?;
    expect_surface_content(&terminal_surface, "terminal")?;

    let web = open_web_window(
        client,
        workspace.id,
        "Window API web",
        "https://example.com/window-api",
    )?;
    let web_tab_id = web.active_tab_id.context("web should return active tab")?;
    close_window_tab(client, web.id, web_tab_id)?;
    let tabs_after_close = list_window_tabs(client, web.id)?;
    if tabs_after_close.iter().any(|tab| tab.id == web_tab_id) {
        bail!("closed web tab should not remain in target window");
    }
    close_window(client, source_window_id)?;
    set_workspace_layout(client, workspace.id, ah_workspace::LayoutMode::Grid)?;

    let events = expect_events(client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(100),
    })?)?;
    require_event_topic(&events, "window")?;
    require_event_topic(&events, "workspace")?;

    println!(
        "workspace={:?} target_window={:?} moved_tab={:?}",
        workspace.id, target.id, source_tab_id
    );
    println!("{}", serde_json::to_string_pretty(&terminal_surface)?);
    Ok(())
}

fn run_session_lifecycle(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use session lifecycle",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Lifecycle terminal")?;
    let session_id = terminal
        .session_id
        .context("lifecycle terminal should have a session")?;

    resize_terminal(client, session_id, 120, 40)?;

    let raw_marker = "AH_LIFECYCLE_RAW_INPUT";
    write_terminal_input(
        client,
        session_id,
        format!("printf '%s\\n' {}\n", shell_quote(raw_marker)),
    )?;
    let raw_surface = wait_for_terminal_tail(client, session_id, raw_marker, DEFAULT_TIMEOUT)?;

    let interrupt_marker = "AH_LIFECYCLE_INTERRUPT_BEGIN";
    run_shell_command(
        client,
        session_id,
        format!(
            "printf '%s\\n' {}; sleep 20; printf '%s\\n' AH_LIFECYCLE_INTERRUPT_DONE",
            shell_quote(interrupt_marker)
        ),
    )?;
    wait_for_terminal_tail(client, session_id, interrupt_marker, DEFAULT_TIMEOUT)?;
    let interrupted = interrupt_session(client, session_id)?;
    if interrupted.ring_state != "error" {
        bail!(
            "interrupted session should enter error ring state, got {}",
            interrupted.ring_state
        );
    }

    let terminated = terminate_session(client, session_id)?;
    if terminated.status != "terminated" {
        bail!(
            "terminated session should report terminated status, got {}",
            terminated.status
        );
    }
    expect_terminal_input_error(client, session_id, "printf 'should-not-write\\n'\n")?;

    let restarted = restart_session(client, session_id)?;
    if restarted.id != session_id {
        bail!("restart should preserve session id");
    }

    let restart_marker = "AH_LIFECYCLE_RESTARTED";
    run_shell_command(
        client,
        session_id,
        format!("printf '%s\\n' {}", shell_quote(restart_marker)),
    )?;
    let restarted_blocks = wait_for_block(client, session_id, restart_marker)?;
    let final_surface =
        expect_surface(client.request(ControlCommand::CaptureSessionSurface { session_id })?)?;
    require_surface_snapshot(&final_surface)?;
    if final_surface.terminal_tail.is_none() {
        bail!("terminal surface should include terminal_tail for Agent observation");
    }

    let events = expect_events(client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(200),
    })?)?;
    require_event_topic(&events, "session")?;

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!(
        "raw_tail_observed={} restarted_blocks={} final_status={}",
        raw_surface
            .terminal_tail
            .as_deref()
            .is_some_and(|tail| tail.contains(raw_marker)),
        restarted_blocks.len(),
        restarted.status
    );
    println!("{}", serde_json::to_string_pretty(&final_surface)?);
    Ok(())
}

fn run_interactive_stdin(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use interactive stdin",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Interactive stdin terminal")?;
    let session_id = terminal
        .session_id
        .context("interactive stdin terminal should have a session")?;

    let marker = "AH_INTERACTIVE_STDIN_20260603";
    run_shell_command(
        client,
        session_id,
        "cat; cat_status=$?; printf 'CAT_EXIT=%s\\n' \"$cat_status\"".to_string(),
    )?;
    write_terminal_input(client, session_id, format!("{marker}\n"))?;
    write_terminal_input(client, session_id, "\x04".to_string())?;
    let blocks = wait_for_block_with_timeout(client, session_id, "CAT_EXIT=0", DEFAULT_TIMEOUT)?;
    let block = blocks
        .first()
        .context("interactive stdin should produce a completed block")?;
    if !block.text.contains(marker) {
        bail!("interactive stdin block should include the input marker");
    }
    if !block.text.contains("CAT_EXIT=0") {
        bail!("interactive stdin block should include cat exit status");
    }

    let surface =
        expect_surface(client.request(ControlCommand::CaptureSessionSurface { session_id })?)?;
    if !surface
        .terminal_tail
        .as_deref()
        .is_some_and(|tail| tail.contains(marker))
    {
        bail!("interactive stdin marker should be visible in terminal tail");
    }

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("matched_blocks={}", blocks.len());
    println!("{}", serde_json::to_string_pretty(&surface)?);
    Ok(())
}

fn run_terminal_grid_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use terminal grid",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Terminal grid smoke")?;
    let session_id = terminal
        .session_id
        .context("terminal grid smoke should have a session")?;

    let marker = "AH_GRID_SCREEN_20260603";
    run_shell_command(
        client,
        session_id,
        format!(
            "printf '\\033[2J\\033[3;7H%s\\033[4;7Hgrid-ready\\n' {}",
            shell_quote(marker)
        ),
    )?;
    let blocks = wait_for_block_with_timeout(client, session_id, marker, DEFAULT_TIMEOUT)?;
    let surface =
        expect_surface(client.request(ControlCommand::CaptureSessionSurface { session_id })?)?;
    let screen = terminal_screen_from_surface_snapshot(&surface)?;
    if !screen.contains(marker) || !screen.contains("grid-ready") {
        bail!("terminal grid snapshot should include ANSI-rendered marker and grid-ready line");
    }

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("matched_blocks={}", blocks.len());
    println!("terminal_screen_contains_marker=true");
    println!("{}", serde_json::to_string_pretty(&surface)?);
    Ok(())
}

fn run_terminal_key_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use terminal key",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Terminal key smoke")?;
    let session_id = terminal
        .session_id
        .context("terminal key smoke should have a session")?;

    let key_probe = r#"import sys
import termios
import tty

fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
keys = []

try:
    tty.setraw(fd)
    sys.stdout.write("AH_KEY_READY\n")
    sys.stdout.flush()
    while len(keys) < 5:
        ch = sys.stdin.read(1)
        if ch == "\x1b":
            seq = ch + sys.stdin.read(2)
            keys.append(seq)
        else:
            keys.append(ch)
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)

sys.stdout.write("\nAH_KEY_OK\n")
for item in keys:
    sys.stdout.write(repr(item) + "\n")
sys.stdout.write("AH_KEY_DONE\n")
sys.stdout.flush()
"#;
    run_shell_command(
        client,
        session_id,
        format!("python3 -c {}", shell_quote(key_probe)),
    )?;
    wait_for_terminal_tail(client, session_id, "AH_KEY_READY", DEFAULT_TIMEOUT)?;

    send_terminal_key(client, session_id, TerminalKeyInput::new("up"))?;
    send_terminal_key(client, session_id, TerminalKeyInput::new("down"))?;
    send_terminal_key(client, session_id, TerminalKeyInput::new("enter"))?;
    send_terminal_key(client, session_id, TerminalKeyInput::new("tab"))?;
    send_terminal_key(
        client,
        session_id,
        TerminalKeyInput {
            key: "c".to_string(),
            text: None,
            modifiers: TerminalKeyInputModifiers {
                control: true,
                ..TerminalKeyInputModifiers::default()
            },
        },
    )?;

    let blocks = wait_for_block_with_timeout(client, session_id, "AH_KEY_DONE", DEFAULT_TIMEOUT)?;
    let block = blocks
        .first()
        .context("terminal key loop should produce a completed block")?;
    for expected in ["'\\x1b[A'", "'\\x1b[B'", "'\\r'", "'\\t'", "'\\x03'"] {
        if !block.text.contains(expected) {
            bail!(
                "terminal key block should include {expected}, got {}",
                block.text
            );
        }
    }

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("matched_blocks={}", blocks.len());
    println!("terminal_keys_observed=true");
    Ok(())
}

fn run_terminal_paste_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use terminal paste",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Terminal paste smoke")?;
    let session_id = terminal
        .session_id
        .context("terminal paste smoke should have a session")?;

    let paste_probe = r#"import sys
import termios
import tty

fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
data = ""

try:
    tty.setraw(fd)
    sys.stdout.write("AH_PASTE_READY\n")
    sys.stdout.flush()
    while "\x1b[200~" not in data:
        data += sys.stdin.read(1)
    payload = ""
    while "\x1b[201~" not in payload:
        payload += sys.stdin.read(1)
    payload = payload.split("\x1b[201~", 1)[0]
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)

sys.stdout.write("\nAH_PASTE_OK\n")
sys.stdout.write(payload)
sys.stdout.write("\nAH_PASTE_DONE\n")
sys.stdout.flush()
"#;
    run_shell_command(
        client,
        session_id,
        format!("python3 -c {}", shell_quote(paste_probe)),
    )?;
    wait_for_terminal_tail(client, session_id, "AH_PASTE_READY", DEFAULT_TIMEOUT)?;

    let payload = "alpha\nbeta\tgamma";
    let paste = paste_sequence_for_text(payload).context("paste payload should not be empty")?;
    write_terminal_input(client, session_id, paste)?;

    let blocks = wait_for_block_with_timeout(client, session_id, "AH_PASTE_DONE", DEFAULT_TIMEOUT)?;
    let block = blocks
        .first()
        .context("terminal paste loop should produce a completed block")?;
    if !block.text.contains("AH_PASTE_OK") || !block.text.contains(payload) {
        bail!("terminal paste block should include bracketed paste payload");
    }

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("matched_blocks={}", blocks.len());
    println!("paste_payload_observed=true");
    Ok(())
}

fn run_terminal_tui_loop(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use terminal TUI",
        Some(default_workspace_root()),
    )?;
    let terminal = open_terminal(client, workspace.id, "Terminal TUI smoke")?;
    let session_id = terminal
        .session_id
        .context("terminal TUI smoke should have a session")?;

    let tui_probe = r#"import sys
import signal
import termios
import tty

fd = sys.stdin.fileno()
old = termios.tcgetattr(fd)
message = "waiting-for-key"

def draw(message):
    sys.stdout.write("\x1b[2J\x1b[3;8HAgentHouse TUI probe\x1b[5;8H")
    sys.stdout.write(message)
    sys.stdout.write("\x1b[7;8HPress q to exit")
    sys.stdout.flush()

def redraw(_signum=None, _frame=None):
    draw(message)

try:
    tty.setraw(fd)
    signal.signal(signal.SIGWINCH, redraw)
    sys.stdout.write("\x1b[?1049h\x1b[H")
    draw(message)
    while True:
        ch = sys.stdin.read(1)
        if ch == "q":
            message = "quit-key"
            draw(message)
            break
        message = "key=" + repr(ch)
        draw(message)
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
    sys.stdout.write("\x1b[?1049l")
    sys.stdout.write("\nAH_TUI_DONE\n")
    sys.stdout.flush()
"#;
    run_shell_command(
        client,
        session_id,
        format!("python3 -c {}", shell_quote(tui_probe)),
    )?;

    let first = wait_for_terminal_snapshot(
        client,
        session_id,
        |snapshot| {
            terminal_snapshot_text(snapshot).contains("AgentHouse TUI probe")
                && terminal_snapshot_text(snapshot).contains("waiting-for-key")
                && snapshot
                    .get("alt_screen")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        },
        DEFAULT_TIMEOUT,
        "TUI alternate screen with waiting marker",
    )?;

    write_terminal_input(client, session_id, "x".to_string())?;
    let after_key = wait_for_terminal_snapshot(
        client,
        session_id,
        |snapshot| terminal_snapshot_text(snapshot).contains("key='x'"),
        DEFAULT_TIMEOUT,
        "TUI screen after x key",
    )?;

    write_terminal_input(client, session_id, "q".to_string())?;
    let blocks = wait_for_block_with_timeout(client, session_id, "AH_TUI_DONE", DEFAULT_TIMEOUT)?;
    let final_snapshot = wait_for_terminal_snapshot(
        client,
        session_id,
        |snapshot| {
            !snapshot
                .get("alt_screen")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
        },
        DEFAULT_TIMEOUT,
        "TUI exit back to main screen",
    )?;

    println!("workspace={:?} session={session_id:?}", workspace.id);
    println!("matched_blocks={}", blocks.len());
    println!(
        "initial_alt_screen={} after_key_contains=true final_alt_screen={}",
        first
            .get("alt_screen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        final_snapshot
            .get("alt_screen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    );
    println!(
        "after_key_lines={}",
        terminal_snapshot_trimmed_lines(&after_key).join(" | ")
    );
    Ok(())
}

fn run_watch_loop(client: &ControlClient) -> Result<()> {
    let snapshot = expect_snapshot(client.request(ControlCommand::Snapshot)?)?;
    let before = expect_events(client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(500),
    })?)?;
    let since_sequence = before.iter().map(|event| event.sequence).max();
    let workspace_id = active_workspace_id(&snapshot)?;
    let terminal = open_terminal(client, workspace_id, "Watch loop terminal")?;
    let session_id = terminal
        .session_id
        .context("watch loop terminal should have a session")?;
    let marker = "AH_WATCH_LOOP";
    run_shell_command(
        client,
        session_id,
        format!("printf '%s\\n' {}", shell_quote(marker)),
    )?;
    let _ = wait_for_block(client, session_id, marker)?;
    let messages = client.collect_events(since_sequence, 8, Duration::from_secs(10))?;
    if !messages.iter().any(|message| {
        matches!(
            message,
            ControlStreamMessage::Event(event) if event.topic == "session"
        )
    }) {
        bail!("watch loop should observe a session event");
    }
    print_json(&serde_json::json!({
        "type": "watch_loop",
        "session_id": session_id,
        "messages": messages,
    }))
}

fn run_persist_write(client: &ControlClient) -> Result<()> {
    let workspace = create_workspace(
        client,
        "AgentHouse-use persistence",
        Some(default_workspace_root()),
    )?;
    set_workspace_layout(client, workspace.id, ah_workspace::LayoutMode::Grid)?;
    let terminal = open_terminal(client, workspace.id, "Persistence terminal")?;
    let session_id = terminal
        .session_id
        .context("persistence terminal should have a session")?;
    let marker = "AgentHouse-persist-marker";
    run_shell_command(
        client,
        session_id,
        format!("printf '%s\\n' {}", shell_quote(marker)),
    )?;
    let blocks = wait_for_block(client, session_id, marker)?;
    let session_before_restart = get_session(client, session_id)?;
    if session_before_restart.unread_count == 0 {
        bail!("persist-write session should have unread ring state before restart");
    }
    let surface = expect_surface(client.request(ControlCommand::CaptureSurface {
        window_id: Some(terminal.id),
    })?)?;
    require_surface_snapshot(&surface)?;
    activate_workspace(client, workspace.id)?;
    println!(
        "persist_write workspace={:?} session={:?} blocks={}",
        workspace.id,
        session_id,
        blocks.len()
    );
    Ok(())
}

fn run_persist_read(client: &ControlClient) -> Result<()> {
    let snapshot = expect_snapshot(client.request(ControlCommand::Snapshot)?)?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.name == "AgentHouse-use persistence")
        .context("persisted workspace should be restored")?;
    if snapshot.active_workspace_id != Some(workspace.id) {
        bail!("persisted active workspace should be restored");
    }
    if workspace.layout_mode != ah_workspace::LayoutMode::Grid {
        bail!("persisted workspace layout should be grid");
    }
    let sessions = list_sessions(client, workspace.id)?;
    let session = sessions
        .iter()
        .find(|session| session.name == "Persistence terminal")
        .context("persisted terminal session should be restored")?;
    if session.unread_count == 0 || session.ring_state != "complete" {
        bail!(
            "persisted ring should restore unread complete state, got state={} unread={}",
            session.ring_state,
            session.unread_count
        );
    }
    let blocks = list_blocks(client, session.id)?;
    if !blocks
        .iter()
        .any(|block| block.text.contains("AgentHouse-persist-marker"))
    {
        bail!("persisted marker block should be restored");
    }
    let events = expect_events(client.request(ControlCommand::ListEvents {
        since_sequence: None,
        limit: Some(200),
    })?)?;
    require_event_topic(&events, "session")?;
    require_event_topic(&events, "store")?;
    println!(
        "persist_read workspace={:?} session={:?} restored_blocks={}",
        workspace.id,
        session.id,
        blocks.len()
    );
    Ok(())
}

fn run_handoff_loop(client: &ControlClient) -> Result<()> {
    let snapshot = expect_snapshot(client.request(ControlCommand::Snapshot)?)?;
    let workspace_id = active_workspace_id(&snapshot)?;

    let source = open_terminal(client, workspace_id, "Handoff source")?;
    let target = open_terminal(client, workspace_id, "Handoff target")?;
    let source_session = source
        .session_id
        .context("source terminal window should have a session")?;
    let target_session = target
        .session_id
        .context("target terminal window should have a session")?;

    let marker = "AgentHouse-handoff-block";
    run_shell_command(
        client,
        source_session,
        format!("printf '%s\\n' {}", shell_quote(marker)),
    )?;
    let source_blocks = wait_for_block(client, source_session, marker)?;
    let source_block = source_blocks
        .first()
        .context("source marker should produce a block")?;

    let forwarded = forward_block(client, source_session, source_block.id, target_session)?;
    if !forwarded.text.contains(marker) {
        bail!("forwarded block should contain source marker text");
    }
    let target_blocks = wait_for_block(client, target_session, marker)?;
    let target_session_after_forward = get_session(client, target_session)?;
    if target_session_after_forward.ring_state != "complete" {
        bail!("target session ring should be complete after handoff");
    }

    println!("source_session={source_session:?} target_session={target_session:?}");
    println!("forwarded_block={:?}", forwarded.id);
    println!("target_blocks_with_marker={}", target_blocks.len());
    println!(
        "{}",
        serde_json::to_string_pretty(&client.request(ControlCommand::CaptureSurface {
            window_id: Some(target.id),
        })?)?
    );
    Ok(())
}

fn run_debate_loop(client: &ControlClient) -> Result<()> {
    let agent = AhUseAgent::new(client);
    let workspace = create_workspace(
        client,
        "AgentHouse-use Claude debate",
        Some(default_workspace_root()),
    )?;
    let affirmative = open_terminal(client, workspace.id, "Claude debate A - all Rust")?;
    let negative = open_terminal(client, workspace.id, "Claude debate B - hybrid")?;
    let affirmative_session = affirmative
        .session_id
        .context("affirmative terminal window should have a session")?;
    let negative_session = negative
        .session_id
        .context("negative terminal window should have a session")?;

    let topic = "AgentHouse 是否应该长期坚持全 Rust 架构，而不是保留 TypeScript/Rust 混合架构";
    let turns = [
        DebateTurn {
            speaker: "A",
            role: "正方",
            session_id: affirmative_session,
            forward_to: negative_session,
            marker: "AH_DEBATE_A1",
            instruction: "先开篇陈述，提出最强的工程理由。",
        },
        DebateTurn {
            speaker: "B",
            role: "反方",
            session_id: negative_session,
            forward_to: affirmative_session,
            marker: "AH_DEBATE_B1",
            instruction: "回应正方观点，并说明混合架构的长期优势。",
        },
        DebateTurn {
            speaker: "A",
            role: "正方",
            session_id: affirmative_session,
            forward_to: negative_session,
            marker: "AH_DEBATE_A2",
            instruction: "针对反方回应做追击，强调 AI-native 原生平台的竞争力。",
        },
        DebateTurn {
            speaker: "B",
            role: "反方",
            session_id: negative_session,
            forward_to: affirmative_session,
            marker: "AH_DEBATE_B2",
            instruction: "最后总结反方立场，并指出正方方案最大的执行风险。",
        },
    ];

    let mut previous_reply = String::from("暂无上一轮观点，请直接开篇。");
    let mut transcript = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        let prompt = debate_prompt(
            topic,
            turn.role,
            turn.instruction,
            &previous_reply,
            turn.marker,
        );
        agent.act_run_claude_stream(turn.session_id, &prompt)?;
        let block = agent.wait_debate_turn(turn.session_id, turn.marker)?;
        let session_before_ack = get_session(client, turn.session_id)?;
        if session_before_ack.unread_count == 0 {
            bail!(
                "debate turn {} should raise a notification before handoff",
                turn.marker
            );
        }
        let reply = extract_claude_result(&block.text, turn.marker);
        let forwarded = agent.act_forward_block(turn.session_id, block.id, turn.forward_to)?;
        let _ = ack_session_ring(client, turn.session_id)?;

        println!(
            "turn={} speaker={} session={:?} forwarded_block={:?}",
            index + 1,
            turn.speaker,
            turn.session_id,
            forwarded.id
        );
        println!("{reply}\n");

        transcript.push(format!("{} {}:\n{}", turn.speaker, turn.role, reply));
        previous_reply = reply;
    }

    let final_surface = client.request(ControlCommand::CaptureSurface {
        window_id: Some(negative.id),
    })?;
    println!("workspace={:?}", workspace.id);
    println!("affirmative_session={affirmative_session:?}");
    println!("negative_session={negative_session:?}");
    println!("debate_transcript=\n{}", transcript.join("\n\n"));
    println!("{}", serde_json::to_string_pretty(&final_surface)?);
    Ok(())
}

fn run_stress_loop(client: &ControlClient, args: &[String]) -> Result<()> {
    let duration = parse_optional_duration_secs(args.first(), Duration::from_secs(1800))?;
    let agent = AhUseAgent::new(client);
    let workspace = agent.create_workspace(
        format!("AgentHouse-use stress {}", unique_request_id()),
        Some(default_workspace_root()),
    )?;
    agent.set_workspace_layout(workspace.id, ah_workspace::LayoutMode::Grid)?;

    let first_window = agent.open_terminal(workspace.id, "Stress terminal A")?;
    let first_session = first_window
        .session_id
        .context("stress terminal A should have a session")?;
    let second_window = agent.create_window(workspace.id, "Stress Window B")?;
    agent.activate_window(workspace.id, second_window.id)?;
    let second_window = agent.open_terminal(workspace.id, "Stress terminal B")?;
    let second_session = second_window
        .session_id
        .context("stress terminal B should have a session")?;
    let web_window = agent.open_web(
        workspace.id,
        "Stress web preview",
        "https://example.com/agenthouse-stress",
    )?;

    let started = Instant::now();
    let mut stats = StressLoopStats::new(duration);
    while started.elapsed() < duration {
        stats.iterations += 1;
        let iteration = stats.iterations;
        let layout = match iteration % 3 {
            0 => ah_workspace::LayoutMode::Single,
            1 => ah_workspace::LayoutMode::Columns,
            _ => ah_workspace::LayoutMode::Grid,
        };
        agent.set_workspace_layout(workspace.id, layout)?;
        stats.layout_changes += 1;

        let source_session = if iteration.is_multiple_of(2) {
            first_session
        } else {
            second_session
        };
        let target_session = if source_session == first_session {
            second_session
        } else {
            first_session
        };
        let marker = format!("AH_STRESS_{iteration:05}");
        let command = format!(
            "printf '%s\\n' {}; printf 'workspace=%s iteration=%s\\n' {} {}",
            shell_quote(&marker),
            shell_quote(&workspace.name),
            iteration
        );
        agent.act_run_shell_command(source_session, command)?;
        stats.commands += 1;
        let blocks =
            agent.wait_block_with_timeout(source_session, &marker, Duration::from_secs(20))?;
        let source_block = blocks
            .first()
            .context("stress command should produce at least one block")?;

        let source_before_ack = agent.get_session(source_session)?;
        if source_before_ack.unread_count == 0 {
            bail!("stress source session should be unread after command completion");
        }
        agent.act_ack_ring(source_session)?;
        stats.ring_acks += 1;

        agent.act_forward_block(source_session, source_block.id, target_session)?;
        stats.forwards += 1;
        agent.wait_block_with_timeout(target_session, &marker, Duration::from_secs(10))?;
        agent.act_ack_ring(target_session)?;
        stats.ring_acks += 1;

        if iteration.is_multiple_of(5) {
            agent.act_resize_terminal(source_session, 120, 36)?;
            stats.resizes += 1;
        }
        if iteration.is_multiple_of(7) {
            let _ = agent.capture_surface(Some(web_window.id))?;
            stats.window_surfaces += 1;
        }
        if iteration.is_multiple_of(3) {
            let _ = agent.inspect_session_surface(source_session)?;
            stats.session_surfaces += 1;
        }

        let events = agent.list_events(None, Some(100))?;
        if !events.iter().any(|event| event.topic == "session") {
            bail!("stress loop should observe session events");
        }
        stats.events_observed += events.len();

        let snapshot = agent.snapshot()?;
        if !snapshot
            .workspaces
            .iter()
            .any(|snapshot_workspace| snapshot_workspace.id == workspace.id)
        {
            bail!("stress workspace disappeared from snapshot");
        }
        if !snapshot
            .sessions
            .iter()
            .any(|session| session.id == first_session)
            || !snapshot
                .sessions
                .iter()
                .any(|session| session.id == second_session)
        {
            bail!("stress sessions disappeared from snapshot");
        }
        stats.snapshots += 1;

        println!(
            "stress iteration={iteration} elapsed={}s commands={} forwards={} events={}",
            started.elapsed().as_secs(),
            stats.commands,
            stats.forwards,
            stats.events_observed
        );
        thread::sleep(Duration::from_millis(250));
    }

    let final_first = agent.inspect_session_surface(first_session)?;
    let final_second = agent.inspect_session_surface(second_session)?;
    stats.elapsed_secs = started.elapsed().as_secs();
    print_json(&serde_json::json!({
        "type": "stress_loop_summary",
        "workspace_id": workspace.id,
        "first_window_id": first_window.id,
        "second_window_id": second_window.id,
        "web_window_id": web_window.id,
        "first_session_id": first_session,
        "second_session_id": second_session,
        "stats": stats,
        "final_first_surface": final_first,
        "final_second_surface": final_second,
    }))
}

pub struct AhUseAgent<'a> {
    client: &'a ControlClient,
}

impl<'a> AhUseAgent<'a> {
    pub fn new(client: &'a ControlClient) -> Self {
        Self { client }
    }

    pub fn snapshot(&self) -> Result<ah_control::ControlSnapshot> {
        expect_snapshot(self.client.request(ControlCommand::Snapshot)?)
    }

    pub fn list_events(
        &self,
        since_sequence: Option<u64>,
        limit: Option<usize>,
    ) -> Result<Vec<ControlEvent>> {
        expect_events(self.client.request(ControlCommand::ListEvents {
            since_sequence,
            limit,
        })?)
    }

    pub fn create_workspace(
        &self,
        name: impl Into<String>,
        root: Option<PathBuf>,
    ) -> Result<WorkspaceSummary> {
        create_workspace(self.client, &name.into(), root)
    }

    pub fn activate_workspace(&self, workspace_id: WorkspaceId) -> Result<()> {
        activate_workspace(self.client, workspace_id)
    }

    pub fn create_window(
        &self,
        workspace_id: WorkspaceId,
        title: impl AsRef<str>,
    ) -> Result<WindowSummary> {
        create_window(self.client, workspace_id, title.as_ref())
    }

    pub fn activate_window(&self, workspace_id: WorkspaceId, window_id: WindowId) -> Result<()> {
        activate_window(self.client, workspace_id, window_id)
    }

    pub fn set_workspace_layout(
        &self,
        workspace_id: WorkspaceId,
        mode: ah_workspace::LayoutMode,
    ) -> Result<()> {
        set_workspace_layout(self.client, workspace_id, mode)
    }

    pub fn list_windows(&self, workspace_id: Option<WorkspaceId>) -> Result<Vec<WindowSummary>> {
        list_windows(self.client, workspace_id)
    }

    pub fn open_terminal(
        &self,
        workspace_id: WorkspaceId,
        title: impl AsRef<str>,
    ) -> Result<WindowSummary> {
        open_terminal(self.client, workspace_id, title.as_ref())
    }

    pub fn open_terminal_tab(
        &self,
        window_id: WindowId,
        title: impl AsRef<str>,
        cwd: Option<PathBuf>,
    ) -> Result<WindowTabSummary> {
        open_terminal_tab(self.client, window_id, title.as_ref(), cwd)
    }

    pub fn open_web(
        &self,
        workspace_id: WorkspaceId,
        title: impl AsRef<str>,
        url: impl AsRef<str>,
    ) -> Result<WindowSummary> {
        open_web_window(self.client, workspace_id, title.as_ref(), url.as_ref())
    }

    pub fn open_web_tab(
        &self,
        window_id: WindowId,
        title: impl AsRef<str>,
        url: impl AsRef<str>,
    ) -> Result<WindowTabSummary> {
        open_web_tab(self.client, window_id, title.as_ref(), url.as_ref())
    }

    pub fn split_window(
        &self,
        window_id: WindowId,
        direction: WindowSplitDirection,
    ) -> Result<WindowSummary> {
        split_window(self.client, window_id, direction)
    }

    pub fn list_window_tabs(&self, window_id: WindowId) -> Result<Vec<WindowTabSummary>> {
        list_window_tabs(self.client, window_id)
    }

    pub fn list_sessions(&self, workspace_id: WorkspaceId) -> Result<Vec<SessionSummary>> {
        list_sessions(self.client, workspace_id)
    }

    pub fn get_session(&self, session_id: SessionId) -> Result<SessionSummary> {
        get_session(self.client, session_id)
    }

    pub fn list_blocks(&self, session_id: SessionId) -> Result<Vec<BlockSummary>> {
        list_blocks(self.client, session_id)
    }

    pub fn capture_surface(&self, window_id: Option<WindowId>) -> Result<SurfaceCapture> {
        expect_surface(
            self.client
                .request(ControlCommand::CaptureSurface { window_id })?,
        )
    }

    pub fn act_run_claude_stream(&self, session_id: SessionId, prompt: &str) -> Result<()> {
        run_claude_prompt_stream(self.client, session_id, prompt)
    }

    pub fn act_run_shell_command(
        &self,
        session_id: SessionId,
        command: impl Into<String>,
    ) -> Result<()> {
        run_shell_command(self.client, session_id, command.into())
    }

    pub fn act_write_terminal_input(
        &self,
        session_id: SessionId,
        input: impl Into<String>,
    ) -> Result<()> {
        write_terminal_input(self.client, session_id, input.into())
    }

    pub fn act_send_terminal_key(
        &self,
        session_id: SessionId,
        key: TerminalKeyInput,
    ) -> Result<()> {
        send_terminal_key(self.client, session_id, key)
    }

    pub fn act_forward_block(
        &self,
        source_session_id: SessionId,
        block_id: BlockId,
        target_session_id: SessionId,
    ) -> Result<BlockSummary> {
        forward_block(self.client, source_session_id, block_id, target_session_id)
    }

    pub fn act_ack_ring(&self, session_id: SessionId) -> Result<SessionSummary> {
        ack_session_ring(self.client, session_id)
    }

    pub fn act_interrupt_session(&self, session_id: SessionId) -> Result<SessionSummary> {
        interrupt_session(self.client, session_id)
    }

    pub fn act_terminate_session(&self, session_id: SessionId) -> Result<SessionSummary> {
        terminate_session(self.client, session_id)
    }

    pub fn act_restart_session(&self, session_id: SessionId) -> Result<SessionSummary> {
        restart_session(self.client, session_id)
    }

    pub fn act_resize_terminal(&self, session_id: SessionId, cols: u16, rows: u16) -> Result<()> {
        resize_terminal(self.client, session_id, cols, rows)
    }

    pub fn wait_debate_turn(&self, session_id: SessionId, marker: &str) -> Result<BlockSummary> {
        wait_for_debate_turn(self.client, session_id, marker)
    }

    pub fn inspect_session_surface(&self, session_id: SessionId) -> Result<SurfaceCapture> {
        expect_surface(
            self.client
                .request(ControlCommand::CaptureSessionSurface { session_id })?,
        )
    }

    pub fn wait_block(&self, session_id: SessionId, marker: &str) -> Result<Vec<BlockSummary>> {
        wait_for_block(self.client, session_id, marker)
    }

    pub fn wait_block_with_timeout(
        &self,
        session_id: SessionId,
        marker: &str,
        timeout: Duration,
    ) -> Result<Vec<BlockSummary>> {
        wait_for_block_with_timeout(self.client, session_id, marker, timeout)
    }

    pub fn wait_ring_state(&self, session_id: SessionId, state: &str) -> Result<SessionSummary> {
        wait_for_ring_state(self.client, session_id, state, DEFAULT_TIMEOUT)
    }

    pub fn wait_event_topic(
        &self,
        topic: &str,
        since_sequence: Option<u64>,
        timeout: Duration,
    ) -> Result<ControlEvent> {
        wait_for_event_topic(self.client, topic, since_sequence, timeout)
    }

    pub fn wait_terminal_tail(
        &self,
        session_id: SessionId,
        marker: &str,
        timeout: Duration,
    ) -> Result<SurfaceCapture> {
        wait_for_terminal_tail(self.client, session_id, marker, timeout)
    }
}

#[derive(Clone, Debug, serde::Serialize)]
struct StressLoopStats {
    requested_duration_secs: u64,
    elapsed_secs: u64,
    iterations: u64,
    commands: u64,
    forwards: u64,
    ring_acks: u64,
    layout_changes: u64,
    resizes: u64,
    window_surfaces: u64,
    session_surfaces: u64,
    snapshots: u64,
    events_observed: usize,
}

impl StressLoopStats {
    fn new(duration: Duration) -> Self {
        Self {
            requested_duration_secs: duration.as_secs(),
            elapsed_secs: 0,
            iterations: 0,
            commands: 0,
            forwards: 0,
            ring_acks: 0,
            layout_changes: 0,
            resizes: 0,
            window_surfaces: 0,
            session_surfaces: 0,
            snapshots: 0,
            events_observed: 0,
        }
    }
}

fn run_claude_command(client: &ControlClient, session_id: SessionId, marker: &str) -> Result<()> {
    let prompt = format!("Respond with exactly this marker and nothing else: {marker}");
    run_claude_prompt(client, session_id, &prompt)
}

fn run_claude_prompt(client: &ControlClient, session_id: SessionId, prompt: &str) -> Result<()> {
    let command = format!(
        "claude -p --output-format json --permission-mode dontAsk --max-budget-usd 0.25 {}",
        shell_quote(prompt)
    );
    let response = client.request(ControlCommand::RunTerminalCommand {
        session_id,
        command,
    })?;

    match response.result {
        ControlResult::CommandQueued { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to queue Claude command: {}", error.message),
        other => bail!("unexpected response to run_terminal_command: {other:?}"),
    }
}

fn run_claude_prompt_stream(
    client: &ControlClient,
    session_id: SessionId,
    prompt: &str,
) -> Result<()> {
    let command = format!(
        "claude --bare -p --verbose --output-format stream-json --include-partial-messages --tools '' --disable-slash-commands --permission-mode dontAsk --max-budget-usd 0.25 {}",
        shell_quote(prompt)
    );
    let response = client.request(ControlCommand::RunTerminalCommand {
        session_id,
        command,
    })?;

    match response.result {
        ControlResult::CommandQueued { .. } => Ok(()),
        ControlResult::Error(error) => {
            bail!(
                "failed to queue streaming Claude command: {}",
                error.message
            )
        }
        other => bail!("unexpected response to run_terminal_command: {other:?}"),
    }
}

fn run_shell_command(client: &ControlClient, session_id: SessionId, command: String) -> Result<()> {
    let response = client.request(ControlCommand::RunTerminalCommand {
        session_id,
        command,
    })?;
    match response.result {
        ControlResult::CommandQueued { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to queue shell command: {}", error.message),
        other => bail!("unexpected response to run_terminal_command: {other:?}"),
    }
}

fn write_terminal_input(
    client: &ControlClient,
    session_id: SessionId,
    input: String,
) -> Result<()> {
    let response = client.request(ControlCommand::WriteTerminalInput { session_id, input })?;
    match response.result {
        ControlResult::TerminalInputWritten { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to write terminal input: {}", error.message),
        other => bail!("unexpected write_terminal_input response: {other:?}"),
    }
}

fn send_terminal_key(
    client: &ControlClient,
    session_id: SessionId,
    key: TerminalKeyInput,
) -> Result<()> {
    let response = client.request(ControlCommand::SendTerminalKey { session_id, key })?;
    match response.result {
        ControlResult::TerminalKeySent { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to send terminal key: {}", error.message),
        other => bail!("unexpected send_terminal_key response: {other:?}"),
    }
}

fn expect_terminal_input_error(
    client: &ControlClient,
    session_id: SessionId,
    input: &str,
) -> Result<()> {
    let response = client.request(ControlCommand::WriteTerminalInput {
        session_id,
        input: input.to_string(),
    })?;
    match response.result {
        ControlResult::Error(_) => Ok(()),
        other => bail!("expected terminal input error, got {other:?}"),
    }
}

fn interrupt_session(client: &ControlClient, session_id: SessionId) -> Result<SessionSummary> {
    let response = client.request(ControlCommand::InterruptSession { session_id })?;
    match response.result {
        ControlResult::SessionInterrupted { session } => Ok(session),
        ControlResult::Error(error) => bail!("failed to interrupt session: {}", error.message),
        other => bail!("unexpected interrupt_session response: {other:?}"),
    }
}

fn terminate_session(client: &ControlClient, session_id: SessionId) -> Result<SessionSummary> {
    let response = client.request(ControlCommand::TerminateSession { session_id })?;
    match response.result {
        ControlResult::SessionTerminated { session } => Ok(session),
        ControlResult::Error(error) => bail!("failed to terminate session: {}", error.message),
        other => bail!("unexpected terminate_session response: {other:?}"),
    }
}

fn restart_session(client: &ControlClient, session_id: SessionId) -> Result<SessionSummary> {
    let response = client.request(ControlCommand::RestartSession { session_id })?;
    match response.result {
        ControlResult::SessionRestarted { session } => Ok(session),
        ControlResult::Error(error) => bail!("failed to restart session: {}", error.message),
        other => bail!("unexpected restart_session response: {other:?}"),
    }
}

fn resize_terminal(
    client: &ControlClient,
    session_id: SessionId,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let response = client.request(ControlCommand::ResizeTerminal {
        session_id,
        cols,
        rows,
    })?;
    match response.result {
        ControlResult::TerminalResized { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to resize terminal: {}", error.message),
        other => bail!("unexpected resize_terminal response: {other:?}"),
    }
}

fn wait_for_claude_block(
    client: &ControlClient,
    session_id: SessionId,
    marker: &str,
) -> Result<Vec<BlockSummary>> {
    wait_for_block(client, session_id, marker)
}

fn wait_for_block(
    client: &ControlClient,
    session_id: SessionId,
    marker: &str,
) -> Result<Vec<BlockSummary>> {
    wait_for_block_with_timeout(client, session_id, marker, DEFAULT_TIMEOUT)
}

fn wait_for_block_with_timeout(
    client: &ControlClient,
    session_id: SessionId,
    marker: &str,
    timeout: Duration,
) -> Result<Vec<BlockSummary>> {
    let started = Instant::now();
    loop {
        let response = client.request(ControlCommand::ListBlocks { session_id })?;
        let blocks = match response.result {
            ControlResult::Blocks { blocks, .. } => blocks,
            ControlResult::Error(error) => bail!("failed to list blocks: {}", error.message),
            other => bail!("unexpected list_blocks response: {other:?}"),
        };

        let matches: Vec<_> = blocks
            .into_iter()
            .filter(|block| block.text.contains(marker) && block.state == "Complete")
            .collect();
        if !matches.is_empty() {
            return Ok(matches);
        }
        if started.elapsed() > timeout {
            bail!("timed out waiting for Claude marker {marker}");
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn wait_for_ring_state(
    client: &ControlClient,
    session_id: SessionId,
    state: &str,
    timeout: Duration,
) -> Result<SessionSummary> {
    let started = Instant::now();
    loop {
        let session = get_session(client, session_id)?;
        if session.ring_state == state {
            return Ok(session);
        }
        if started.elapsed() > timeout {
            bail!(
                "timed out waiting for session {session_id:?} ring state {state}; current={}",
                session.ring_state
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_event_topic(
    client: &ControlClient,
    topic: &str,
    mut since_sequence: Option<u64>,
    timeout: Duration,
) -> Result<ControlEvent> {
    let started = Instant::now();
    loop {
        let events = expect_events(client.request(ControlCommand::ListEvents {
            since_sequence,
            limit: Some(100),
        })?)?;
        if let Some(event) = events.iter().find(|event| event.topic == topic) {
            return Ok(event.clone());
        }
        if let Some(max_sequence) = events.iter().map(|event| event.sequence).max() {
            since_sequence = Some(max_sequence);
        }
        if started.elapsed() > timeout {
            bail!("timed out waiting for event topic {topic}");
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_terminal_tail(
    client: &ControlClient,
    session_id: SessionId,
    marker: &str,
    timeout: Duration,
) -> Result<SurfaceCapture> {
    let started = Instant::now();
    loop {
        let surface =
            expect_surface(client.request(ControlCommand::CaptureSessionSurface { session_id })?)?;
        if surface
            .terminal_tail
            .as_deref()
            .is_some_and(|tail| tail.contains(marker))
        {
            return Ok(surface);
        }
        if started.elapsed() > timeout {
            bail!("timed out waiting for terminal tail marker {marker}");
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_web_preview(
    client: &ControlClient,
    window_id: WindowId,
    url: &str,
    timeout: Duration,
) -> Result<SurfaceCapture> {
    let started = Instant::now();
    loop {
        let surface = expect_surface(client.request(ControlCommand::CaptureSurface {
            window_id: Some(window_id),
        })?)?;
        expect_surface_content(&surface, "web")?;
        if surface.target_url.as_deref() != Some(url) {
            bail!("web surface should expose target_url {url}");
        }
        require_surface_snapshot(&surface)?;
        let snapshot = surface_snapshot_value(&surface)?;
        let browser_status = snapshot
            .get("browser_session")
            .and_then(|session| session.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let web_preview = snapshot
            .get("web_preview")
            .context("web surface snapshot should expose web_preview")?;
        if let Some(error) = web_preview.get("error").and_then(serde_json::Value::as_str) {
            bail!("web preview failed: {error}");
        }
        let preview_status = web_preview
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let page_url = snapshot
            .get("browser_surface")
            .and_then(|surface| surface.get("page"))
            .and_then(|page| page.get("url"))
            .and_then(serde_json::Value::as_str);
        let has_page = page_url == Some(url);
        let is_ready = has_page && browser_status == "Loaded";
        if is_ready {
            return Ok(surface);
        }
        if started.elapsed() > timeout {
            bail!(
                "timed out waiting for web preview {url}; browser={browser_status} preview={preview_status} page_url={page_url:?}"
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_terminal_snapshot<F>(
    client: &ControlClient,
    session_id: SessionId,
    mut predicate: F,
    timeout: Duration,
    description: &str,
) -> Result<serde_json::Value>
where
    F: FnMut(&serde_json::Value) -> bool,
{
    let started = Instant::now();
    loop {
        let surface =
            expect_surface(client.request(ControlCommand::CaptureSessionSurface { session_id })?)?;
        let snapshot = terminal_snapshot_from_surface_snapshot(&surface)?;
        if predicate(&snapshot) {
            return Ok(snapshot);
        }
        if started.elapsed() > timeout {
            bail!("timed out waiting for terminal snapshot: {description}");
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn wait_for_debate_turn(
    client: &ControlClient,
    session_id: SessionId,
    marker: &str,
) -> Result<BlockSummary> {
    let started = Instant::now();
    loop {
        let session = get_session(client, session_id)?;
        let blocks = list_blocks(client, session_id)?;

        if let Some(block) = blocks
            .iter()
            .find(|block| block.state == "Complete" && block.text.contains(marker))
            .cloned()
        {
            return Ok(block);
        }

        if session.ring_state == "complete"
            && let Some(block) = blocks
                .iter()
                .find(|block| block.state == "Complete")
                .cloned()
        {
            return Ok(block);
        }

        if session.ring_state == "error" {
            bail!(
                "debate turn failed before marker {marker}; status={}; ring={}",
                session.status,
                session.ring_summary
            );
        }

        if started.elapsed() > DEBATE_TURN_TIMEOUT {
            bail!(
                "timed out waiting for debate marker {marker}; status={}; ring_state={}; ring_summary={}",
                session.status,
                session.ring_state,
                session.ring_summary
            );
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn debate_prompt(
    topic: &str,
    role: &str,
    instruction: &str,
    previous_reply: &str,
    marker: &str,
) -> String {
    format!(
        "你是 AgentHouse 双会话稳定性测试中的 Claude 辩手，立场：{role}。\n\
辩题：{topic}\n\
上一轮对方观点：\n{previous_reply}\n\n\
任务：{instruction}\n\
要求：用中文，控制在 80 到 130 字，最多 3 个要点，聚焦工程判断，不要解释测试机制。不要使用工具，不要搜索，不要读取文件，只直接回答。最后单独一行输出标记 {marker}。"
    )
}

fn extract_claude_result(block_text: &str, marker: &str) -> String {
    let mut partial_text = String::new();
    let mut assistant_text = String::new();
    for value in parse_json_objects(block_text) {
        if let Some(result) = value.get("result").and_then(serde_json::Value::as_str) {
            return strip_marker(result, marker);
        }
        collect_assistant_message_text(&value, &mut assistant_text);
        collect_stream_delta_text(&value, &mut partial_text);
    }
    if !assistant_text.trim().is_empty() {
        strip_marker(&assistant_text, marker)
    } else if !partial_text.trim().is_empty() {
        strip_marker(&partial_text, marker)
    } else {
        strip_marker(block_text, marker)
    }
}

fn parse_json_objects(text: &str) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let mut object = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    let mut collecting = false;

    for ch in text.chars() {
        if !collecting {
            if ch == '{' {
                collecting = true;
                depth = 1;
                object.clear();
                object.push(ch);
            }
            continue;
        }

        if in_string {
            if escape {
                object.push(ch);
                escape = false;
            } else if ch == '\\' {
                object.push(ch);
                escape = true;
            } else if ch == '"' {
                object.push(ch);
                in_string = false;
            } else if ch != '\n' && ch != '\r' {
                object.push(ch);
            }
            continue;
        }

        match ch {
            '"' => {
                object.push(ch);
                in_string = true;
            }
            '{' => {
                object.push(ch);
                depth = depth.saturating_add(1);
            }
            '}' => {
                object.push(ch);
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&object) {
                        values.push(value);
                    }
                    collecting = false;
                    object.clear();
                }
            }
            _ => object.push(ch),
        }
    }

    values
}

fn collect_assistant_message_text(value: &serde_json::Value, output: &mut String) {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("assistant") {
        return;
    }
    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    for item in content {
        if item.get("type").and_then(serde_json::Value::as_str) == Some("text")
            && let Some(text) = item.get("text").and_then(serde_json::Value::as_str)
        {
            output.push_str(text);
        }
    }
}

fn collect_stream_delta_text(value: &serde_json::Value, output: &mut String) {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("stream_event") {
        return;
    }
    let Some(delta) = value
        .get("event")
        .and_then(|event| event.get("delta"))
        .and_then(|delta| delta.get("text"))
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    output.push_str(delta);
}

fn strip_marker(text: &str, marker: &str) -> String {
    let mut cleaned = String::new();
    for line in text.replace(marker, "").lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !cleaned.is_empty() {
            cleaned.push('\n');
        }
        cleaned.push_str(line);
    }
    cleaned
}

#[derive(Clone, Copy, Debug)]
struct DebateTurn {
    speaker: &'static str,
    role: &'static str,
    session_id: SessionId,
    forward_to: SessionId,
    marker: &'static str,
    instruction: &'static str,
}

fn create_workspace(
    client: &ControlClient,
    name: &str,
    root: Option<PathBuf>,
) -> Result<WorkspaceSummary> {
    let response = client.request(ControlCommand::CreateWorkspace {
        name: name.to_string(),
        root,
    })?;
    match response.result {
        ControlResult::WorkspaceCreated { workspace } => Ok(workspace),
        ControlResult::Error(error) => bail!("failed to create workspace: {}", error.message),
        other => bail!("unexpected create_workspace response: {other:?}"),
    }
}

fn activate_workspace(client: &ControlClient, workspace_id: WorkspaceId) -> Result<()> {
    let response = client.request(ControlCommand::ActivateWorkspace { workspace_id })?;
    match response.result {
        ControlResult::WorkspaceActivated { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to activate workspace: {}", error.message),
        other => bail!("unexpected activate_workspace response: {other:?}"),
    }
}

fn activate_window(
    client: &ControlClient,
    workspace_id: WorkspaceId,
    window_id: WindowId,
) -> Result<()> {
    let response = client.request(ControlCommand::ActivateWindow {
        workspace_id,
        window_id,
    })?;
    match response.result {
        ControlResult::WindowActivated { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to activate window: {}", error.message),
        other => bail!("unexpected activate_window response: {other:?}"),
    }
}

fn list_windows(
    client: &ControlClient,
    workspace_id: Option<WorkspaceId>,
) -> Result<Vec<WindowSummary>> {
    let response = client.request(ControlCommand::ListWindows { workspace_id })?;
    match response.result {
        ControlResult::Windows { windows } => Ok(windows),
        ControlResult::Error(error) => bail!("failed to list windows: {}", error.message),
        other => bail!("unexpected list_windows response: {other:?}"),
    }
}

fn open_web_window(
    client: &ControlClient,
    workspace_id: WorkspaceId,
    title: &str,
    url: &str,
) -> Result<WindowSummary> {
    let response = client.request(ControlCommand::OpenWebWindow {
        workspace_id,
        title: title.to_string(),
        url: url.to_string(),
    })?;
    match response.result {
        ControlResult::WindowOpened { window, .. } => Ok(window),
        ControlResult::Error(error) => bail!("failed to open web window: {}", error.message),
        other => bail!("unexpected open_web_window response: {other:?}"),
    }
}

fn open_terminal_tab(
    client: &ControlClient,
    window_id: WindowId,
    title: &str,
    cwd: Option<PathBuf>,
) -> Result<WindowTabSummary> {
    let response = client.request(ControlCommand::OpenTerminalTab {
        window_id,
        title: title.to_string(),
        cwd,
    })?;
    match response.result {
        ControlResult::WindowTabOpened { tab, .. } => Ok(tab),
        ControlResult::Error(error) => bail!("failed to open terminal tab: {}", error.message),
        other => bail!("unexpected open_terminal_tab response: {other:?}"),
    }
}

fn open_web_tab(
    client: &ControlClient,
    window_id: WindowId,
    title: &str,
    url: &str,
) -> Result<WindowTabSummary> {
    let response = client.request(ControlCommand::OpenWebTab {
        window_id,
        title: title.to_string(),
        url: url.to_string(),
    })?;
    match response.result {
        ControlResult::WindowTabOpened { tab, .. } => Ok(tab),
        ControlResult::Error(error) => bail!("failed to open web tab: {}", error.message),
        other => bail!("unexpected open_web_tab response: {other:?}"),
    }
}

fn split_window(
    client: &ControlClient,
    window_id: WindowId,
    direction: WindowSplitDirection,
) -> Result<WindowSummary> {
    let response = client.request(ControlCommand::SplitWindow {
        window_id,
        direction,
    })?;
    match response.result {
        ControlResult::WindowSplit { window, .. } => Ok(window),
        ControlResult::Error(error) => bail!("failed to split window: {}", error.message),
        other => bail!("unexpected split_window response: {other:?}"),
    }
}

fn create_window(
    client: &ControlClient,
    workspace_id: WorkspaceId,
    title: &str,
) -> Result<WindowSummary> {
    let response = client.request(ControlCommand::CreateWindow {
        workspace_id,
        title: title.to_string(),
    })?;
    match response.result {
        ControlResult::WindowCreated { window, .. } => Ok(window),
        ControlResult::Error(error) => bail!("failed to create window: {}", error.message),
        other => bail!("unexpected create_window response: {other:?}"),
    }
}

fn close_window(client: &ControlClient, window_id: WindowId) -> Result<()> {
    let response = client.request(ControlCommand::CloseWindow { window_id })?;
    match response.result {
        ControlResult::WindowClosed { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to close window: {}", error.message),
        other => bail!("unexpected close_window response: {other:?}"),
    }
}

fn close_window_tab(client: &ControlClient, window_id: WindowId, tab_id: TabId) -> Result<()> {
    let response = client.request(ControlCommand::CloseWindowTab { window_id, tab_id })?;
    match response.result {
        ControlResult::WindowTabClosed { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to close window tab: {}", error.message),
        other => bail!("unexpected close_window_tab response: {other:?}"),
    }
}

fn move_window_tab(
    client: &ControlClient,
    source_window_id: WindowId,
    tab_id: TabId,
    target_window_id: WindowId,
) -> Result<()> {
    let response = client.request(ControlCommand::MoveWindowTab {
        source_window_id,
        tab_id,
        target_window_id,
    })?;
    match response.result {
        ControlResult::WindowTabMoved { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to move window tab: {}", error.message),
        other => bail!("unexpected move_window_tab response: {other:?}"),
    }
}

fn set_workspace_layout(
    client: &ControlClient,
    workspace_id: WorkspaceId,
    mode: ah_workspace::LayoutMode,
) -> Result<()> {
    let response = client.request(ControlCommand::SetWorkspaceLayout { workspace_id, mode })?;
    match response.result {
        ControlResult::WorkspaceLayoutSet { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to set workspace layout: {}", error.message),
        other => bail!("unexpected set_workspace_layout response: {other:?}"),
    }
}

fn list_window_tabs(client: &ControlClient, window_id: WindowId) -> Result<Vec<WindowTabSummary>> {
    let response = client.request(ControlCommand::ListWindowTabs { window_id })?;
    match response.result {
        ControlResult::WindowTabs { tabs, .. } => Ok(tabs),
        ControlResult::Error(error) => bail!("failed to list window tabs: {}", error.message),
        other => bail!("unexpected list_window_tabs response: {other:?}"),
    }
}

fn require_tab(tabs: &[WindowTabSummary], tab_id: TabId, content_type: &str) -> Result<()> {
    if tabs
        .iter()
        .any(|tab| tab.id == tab_id && tab.content_type == content_type)
    {
        Ok(())
    } else {
        bail!("expected tab {tab_id:?} with content_type {content_type}")
    }
}

fn activate_window_tab(client: &ControlClient, window_id: WindowId, tab_id: TabId) -> Result<()> {
    let response = client.request(ControlCommand::ActivateWindowTab { window_id, tab_id })?;
    match response.result {
        ControlResult::WindowTabActivated { .. } => Ok(()),
        ControlResult::Error(error) => bail!("failed to activate window tab: {}", error.message),
        other => bail!("unexpected activate_window_tab response: {other:?}"),
    }
}

fn get_session(client: &ControlClient, session_id: SessionId) -> Result<SessionSummary> {
    let response = client.request(ControlCommand::GetSession { session_id })?;
    match response.result {
        ControlResult::Session { session } => Ok(session),
        ControlResult::Error(error) => bail!("failed to get session: {}", error.message),
        other => bail!("unexpected get_session response: {other:?}"),
    }
}

fn ack_session_ring(client: &ControlClient, session_id: SessionId) -> Result<SessionSummary> {
    let response = client.request(ControlCommand::AckSessionRing { session_id })?;
    match response.result {
        ControlResult::SessionRingAcknowledged { session } => Ok(session),
        ControlResult::Error(error) => bail!("failed to ack session ring: {}", error.message),
        other => bail!("unexpected ack_session_ring response: {other:?}"),
    }
}

fn list_blocks(client: &ControlClient, session_id: SessionId) -> Result<Vec<BlockSummary>> {
    let response = client.request(ControlCommand::ListBlocks { session_id })?;
    match response.result {
        ControlResult::Blocks { blocks, .. } => Ok(blocks),
        ControlResult::Error(error) => bail!("failed to list blocks: {}", error.message),
        other => bail!("unexpected list_blocks response: {other:?}"),
    }
}

fn list_sessions(client: &ControlClient, workspace_id: WorkspaceId) -> Result<Vec<SessionSummary>> {
    let response = client.request(ControlCommand::ListSessions {
        workspace_id: Some(workspace_id),
    })?;
    match response.result {
        ControlResult::Sessions { sessions } => Ok(sessions),
        ControlResult::Error(error) => bail!("failed to list sessions: {}", error.message),
        other => bail!("unexpected list_sessions response: {other:?}"),
    }
}

fn forward_block(
    client: &ControlClient,
    source_session_id: SessionId,
    block_id: ah_core::BlockId,
    target_session_id: SessionId,
) -> Result<BlockSummary> {
    let response = client.request(ControlCommand::ForwardBlock {
        source_session_id,
        block_id,
        target_session_id,
    })?;
    match response.result {
        ControlResult::BlockForwarded { block, .. } => Ok(block),
        ControlResult::Error(error) => bail!("failed to forward block: {}", error.message),
        other => bail!("unexpected forward_block response: {other:?}"),
    }
}

fn expect_surface(response: ControlResponse) -> Result<SurfaceCapture> {
    match response.result {
        ControlResult::SurfaceCapture(surface) => Ok(*surface),
        ControlResult::Error(error) => bail!("surface capture failed: {}", error.message),
        other => bail!("unexpected surface capture response: {other:?}"),
    }
}

fn expect_surface_content(surface: &SurfaceCapture, content_type: &str) -> Result<()> {
    if surface.content_type.as_deref() == Some(content_type) {
        Ok(())
    } else {
        bail!(
            "expected surface content_type {content_type}, got {:?}",
            surface.content_type
        )
    }
}

fn require_surface_snapshot(surface: &SurfaceCapture) -> Result<()> {
    let path = surface
        .snapshot_path
        .as_ref()
        .context("surface should include app-generated snapshot_path")?;
    if !path.exists() {
        bail!("surface snapshot_path does not exist: {}", path.display());
    }
    Ok(())
}

fn surface_snapshot_value(surface: &SurfaceCapture) -> Result<serde_json::Value> {
    let path = surface
        .snapshot_path
        .as_ref()
        .context("surface should include app-generated snapshot_path")?;
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read surface snapshot {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse surface snapshot {}", path.display()))
}

fn terminal_screen_from_surface_snapshot(surface: &SurfaceCapture) -> Result<String> {
    let value = surface_snapshot_value(surface)?;
    value
        .get("terminal_screen")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .context("surface snapshot should include terminal_screen")
}

fn terminal_snapshot_from_surface_snapshot(surface: &SurfaceCapture) -> Result<serde_json::Value> {
    let value = surface_snapshot_value(surface)?;
    value
        .get("terminal_snapshot")
        .cloned()
        .context("surface snapshot should include terminal_snapshot")
}

fn terminal_snapshot_text(snapshot: &serde_json::Value) -> String {
    snapshot
        .get("lines")
        .and_then(serde_json::Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(terminal_snapshot_line_text)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn terminal_snapshot_line_text(line: &serde_json::Value) -> String {
    line.get("cells")
        .and_then(serde_json::Value::as_array)
        .map(|cells| {
            cells
                .iter()
                .filter_map(|cell| cell.get("ch").and_then(serde_json::Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn terminal_snapshot_trimmed_lines(snapshot: &serde_json::Value) -> Vec<String> {
    snapshot
        .get("lines")
        .and_then(serde_json::Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(terminal_snapshot_line_text)
                .map(|line| line.trim_end().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn terminal_snapshot_looks_like_claude_tui(snapshot: &serde_json::Value) -> bool {
    let text = terminal_snapshot_text(snapshot);
    let lower = text.to_lowercase();
    let has_claude_identity =
        lower.contains("claude") || lower.contains("anthropic") || lower.contains("claude code");
    let has_interactive_surface = lower.contains("welcome")
        || lower.contains("login")
        || lower.contains("continue")
        || lower.contains("permission")
        || lower.contains("trust")
        || lower.contains("cwd")
        || lower.contains("help")
        || lower.contains("message")
        || lower.contains("prompt")
        || snapshot
            .get("alt_screen")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

    has_claude_identity && has_interactive_surface
}

fn terminal_snapshot_looks_like_claude_trust_prompt(snapshot: &serde_json::Value) -> bool {
    text_looks_like_claude_trust_prompt(&terminal_snapshot_text(snapshot))
}

fn terminal_snapshot_looks_like_claude_after_trust(snapshot: &serde_json::Value) -> bool {
    terminal_snapshot_looks_like_claude_tui(snapshot)
        && !terminal_snapshot_looks_like_claude_trust_prompt(snapshot)
}

fn text_looks_like_claude_trust_prompt(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("quick safety check")
        || lower.contains("trust this folder")
        || (lower.contains("accessing workspace") && lower.contains("yes, i trust"))
}

fn expect_events(response: ControlResponse) -> Result<Vec<ControlEvent>> {
    match response.result {
        ControlResult::Events { events } => Ok(events),
        ControlResult::Error(error) => bail!("list_events failed: {}", error.message),
        other => bail!("unexpected list_events response: {other:?}"),
    }
}

fn require_event_topic(events: &[ControlEvent], topic: &str) -> Result<()> {
    if events.iter().any(|event| event.topic == topic) {
        Ok(())
    } else {
        bail!("expected event topic {topic} in recent events")
    }
}

fn open_terminal(
    client: &ControlClient,
    workspace_id: WorkspaceId,
    title: &str,
) -> Result<WindowSummary> {
    let response = client.request(ControlCommand::OpenTerminalWindow {
        workspace_id,
        title: title.to_string(),
        cwd: Some(default_workspace_root()),
    })?;
    match response.result {
        ControlResult::WindowOpened { window, .. } => Ok(window),
        ControlResult::Error(error) => bail!("failed to open terminal window: {}", error.message),
        other => bail!("unexpected open_terminal_window response: {other:?}"),
    }
}

fn expect_snapshot(response: ControlResponse) -> Result<ah_control::ControlSnapshot> {
    match response.result {
        ControlResult::Snapshot(snapshot) => Ok(snapshot),
        ControlResult::Error(error) => bail!("snapshot failed: {}", error.message),
        other => bail!("unexpected snapshot response: {other:?}"),
    }
}

fn active_workspace_id(snapshot: &ah_control::ControlSnapshot) -> Result<WorkspaceId> {
    snapshot
        .active_workspace_id
        .or_else(|| snapshot.workspaces.first().map(|workspace| workspace.id))
        .context("snapshot should contain a workspace")
}

fn first_terminal_session(windows: &[WindowSummary]) -> Result<SessionId> {
    windows
        .iter()
        .find(|window| window.content_type == "terminal")
        .and_then(|window| window.session_id)
        .context("snapshot should contain a terminal session")
}

fn shell_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn required_arg<'a>(arg: Option<&'a String>, name: &str) -> Result<&'a str> {
    arg.map(String::as_str)
        .with_context(|| format!("missing required argument: {name}"))
}

fn parse_id<T>(value: &str, name: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .with_context(|| format!("failed to parse {name}: {value}"))
}

fn parse_required_workspace_id(arg: Option<&String>, name: &str) -> Result<WorkspaceId> {
    parse_id(required_arg(arg, name)?, name)
}

fn parse_optional_workspace_id(arg: Option<&String>) -> Result<Option<WorkspaceId>> {
    arg.map(|value| parse_id(value, "workspace id")).transpose()
}

fn parse_required_window_id(arg: Option<&String>, name: &str) -> Result<WindowId> {
    parse_id(required_arg(arg, name)?, name)
}

fn parse_optional_window_id(arg: Option<&String>) -> Result<Option<WindowId>> {
    arg.map(|value| parse_id(value, "window id")).transpose()
}

fn parse_window_split_direction(arg: Option<&String>) -> Result<WindowSplitDirection> {
    match required_arg(arg, "split direction")? {
        "right" => Ok(WindowSplitDirection::Right),
        "down" => Ok(WindowSplitDirection::Down),
        value => bail!("unsupported split direction: {value}; expected right or down"),
    }
}

fn parse_ui_language_preference(arg: Option<&String>) -> Result<UiLanguagePreference> {
    let value = required_arg(arg, "UI language")?;
    UiLanguagePreference::from_control_code(value)
        .with_context(|| format!("unsupported UI language: {value}; expected zh-cn or en"))
}

fn parse_ui_theme_scheme_preference(arg: Option<&String>) -> Result<UiThemeSchemePreference> {
    let value = required_arg(arg, "UI color scheme")?;
    UiThemeSchemePreference::from_control_code(value)
        .with_context(|| format!("unsupported UI color scheme: {value}; expected glass"))
}

fn parse_ui_theme_mode_preference(arg: Option<&String>) -> Result<UiThemeModePreference> {
    let value = required_arg(arg, "UI theme mode")?;
    UiThemeModePreference::from_control_code(value).with_context(|| {
        format!("unsupported UI theme mode: {value}; expected system, light, or dark")
    })
}

fn parse_required_tab_id(arg: Option<&String>, name: &str) -> Result<TabId> {
    parse_id(required_arg(arg, name)?, name)
}

fn parse_required_session_id(arg: Option<&String>, name: &str) -> Result<SessionId> {
    parse_id(required_arg(arg, name)?, name)
}

fn parse_required_block_id(arg: Option<&String>, name: &str) -> Result<BlockId> {
    parse_id(required_arg(arg, name)?, name)
}

fn parse_optional_u64(arg: Option<&String>, name: &str) -> Result<Option<u64>> {
    arg.map(|value| {
        value
            .parse()
            .with_context(|| format!("failed to parse {name}: {value}"))
    })
    .transpose()
}

fn parse_optional_usize(arg: Option<&String>, name: &str) -> Result<Option<usize>> {
    arg.map(|value| {
        value
            .parse()
            .with_context(|| format!("failed to parse {name}: {value}"))
    })
    .transpose()
}

fn parse_required_u16(arg: Option<&String>, name: &str) -> Result<u16> {
    let value = required_arg(arg, name)?;
    value
        .parse()
        .with_context(|| format!("failed to parse {name}: {value}"))
}

fn parse_optional_duration_secs(arg: Option<&String>, default: Duration) -> Result<Duration> {
    let Some(value) = arg else {
        return Ok(default);
    };
    let seconds: u64 = value
        .parse()
        .with_context(|| format!("failed to parse timeout seconds: {value}"))?;
    Ok(Duration::from_secs(seconds))
}

fn parse_terminal_key_args(
    key_arg: Option<&String>,
    modifiers_arg: Option<&String>,
) -> Result<TerminalKeyInput> {
    let key = required_arg(key_arg, "terminal key")?.to_string();
    let modifiers = parse_terminal_key_modifiers(modifiers_arg)?;
    let text = terminal_key_text_for_cli(&key, &modifiers);
    Ok(TerminalKeyInput {
        key,
        text,
        modifiers,
    })
}

fn parse_terminal_key_modifiers(arg: Option<&String>) -> Result<TerminalKeyInputModifiers> {
    let mut modifiers = TerminalKeyInputModifiers::default();
    let Some(value) = arg else {
        return Ok(modifiers);
    };
    for token in value
        .split([',', '+'])
        .map(str::trim)
        .filter(|token| !token.is_empty() && *token != "none")
    {
        match token {
            "alt" | "option" => modifiers.alt = true,
            "control" | "ctrl" => modifiers.control = true,
            "shift" => modifiers.shift = true,
            "platform" | "cmd" | "command" | "super" => modifiers.platform = true,
            other => bail!("unknown terminal key modifier: {other}"),
        }
    }
    Ok(modifiers)
}

fn terminal_key_text_for_cli(key: &str, modifiers: &TerminalKeyInputModifiers) -> Option<String> {
    if modifiers.control || modifiers.platform {
        return None;
    }
    if key == "space" {
        return Some(" ".to_string());
    }
    if key.chars().count() == 1 {
        Some(key.to_string())
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct ControlClient {
    socket_path: PathBuf,
}

impl Default for ControlClient {
    fn default() -> Self {
        Self {
            socket_path: std::env::temp_dir().join(ah_control::DEFAULT_SOCKET_NAME),
        }
    }
}

impl ControlClient {
    #[must_use]
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    #[must_use]
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn request(&self, command: ControlCommand) -> Result<ControlResponse> {
        let id = unique_request_id();
        let request = ControlRequest { id, command };
        let mut stream = UnixStream::connect(&self.socket_path)
            .with_context(|| format!("failed to connect {}", self.socket_path.display()))?;

        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        let response = serde_json::from_str(&line)?;
        Ok(response)
    }

    pub fn watch_events(&self, since_sequence: Option<u64>, max_messages: usize) -> Result<()> {
        for message in self.collect_events(since_sequence, max_messages, DEFAULT_TIMEOUT)? {
            println!("{}", serde_json::to_string(&message)?);
        }
        Ok(())
    }

    pub fn collect_events(
        &self,
        since_sequence: Option<u64>,
        max_messages: usize,
        timeout: Duration,
    ) -> Result<Vec<ControlStreamMessage>> {
        let id = unique_request_id();
        let request = ControlRequest {
            id,
            command: ControlCommand::WatchEvents {
                since_sequence,
                limit: Some(100),
            },
        };
        let mut stream = UnixStream::connect(&self.socket_path)
            .with_context(|| format!("failed to connect {}", self.socket_path.display()))?;

        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        stream.set_read_timeout(Some(timeout))?;

        let mut messages = Vec::new();
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            if messages.len() >= max_messages {
                break;
            }
            let line = line?;
            let message: ControlStreamMessage = serde_json::from_str(&line)?;
            let is_event = matches!(message, ControlStreamMessage::Event(_));
            messages.push(message);
            if is_event && messages.len() >= max_messages {
                break;
            }
        }
        Ok(messages)
    }
}

fn unique_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    format!("ah-use-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_result_from_wrapped_json_object() {
        let block = concat!(
            "{\"type\":\"result\",\"result\":\"hello\\nAH_DEBATE_A1\",\"usage\":{\"input_tokens\":1",
            "\n,\"output_tokens\":2}}\n78\n"
        );

        assert_eq!(extract_claude_result(block, "AH_DEBATE_A1"), "hello");
    }

    #[test]
    fn extracts_assistant_text_from_wrapped_json_object() {
        let block = concat!(
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"first ",
            "\nline\\nAH_DEBATE_B1\"}]}}\n"
        );

        assert_eq!(extract_claude_result(block, "AH_DEBATE_B1"), "first line");
    }

    #[test]
    fn parses_terminal_key_cli_args() {
        let key = parse_terminal_key_args(Some(&"a".to_string()), None)
            .expect("plain text key should parse");
        assert_eq!(key.key, "a");
        assert_eq!(key.text.as_deref(), Some("a"));
        assert_eq!(key.modifiers, TerminalKeyInputModifiers::default());

        let space = parse_terminal_key_args(Some(&"space".to_string()), None)
            .expect("space key should parse");
        assert_eq!(space.text.as_deref(), Some(" "));

        let ctrl_c = parse_terminal_key_args(Some(&"c".to_string()), Some(&"ctrl".to_string()))
            .expect("ctrl modifier should parse");
        assert_eq!(ctrl_c.key, "c");
        assert!(ctrl_c.modifiers.control);
        assert_eq!(ctrl_c.text, None);

        let shifted_tab =
            parse_terminal_key_args(Some(&"tab".to_string()), Some(&"shift".to_string()))
                .expect("shift modifier should parse");
        assert_eq!(shifted_tab.key, "tab");
        assert!(shifted_tab.modifiers.shift);
    }

    #[test]
    fn parses_ui_language_cli_args() {
        assert_eq!(
            parse_ui_language_preference(Some(&"\u{4e2d}\u{6587}".to_string())).unwrap(),
            UiLanguagePreference::ZhCn
        );
        assert_eq!(
            parse_ui_language_preference(Some(&"en".to_string())).unwrap(),
            UiLanguagePreference::En
        );
        assert!(parse_ui_language_preference(Some(&"fr".to_string())).is_err());
    }

    #[test]
    fn parses_ui_theme_scheme_cli_args() {
        assert_eq!(
            parse_ui_theme_scheme_preference(Some(&"\u{6742}\u{5fd7}".to_string())).unwrap(),
            UiThemeSchemePreference::Glass
        );
        assert_eq!(
            parse_ui_theme_scheme_preference(Some(&"glass".to_string())).unwrap(),
            UiThemeSchemePreference::Glass
        );
        assert!(parse_ui_theme_scheme_preference(Some(&"neon".to_string())).is_err());
    }

    #[test]
    fn parses_ui_theme_mode_cli_args() {
        assert_eq!(
            parse_ui_theme_mode_preference(Some(&"\u{6df1}\u{8272}".to_string())).unwrap(),
            UiThemeModePreference::Dark
        );
        assert_eq!(
            parse_ui_theme_mode_preference(Some(&"system".to_string())).unwrap(),
            UiThemeModePreference::System
        );
        assert!(parse_ui_theme_mode_preference(Some(&"sepia".to_string())).is_err());
    }

    #[test]
    fn detects_claude_trust_prompt_text() {
        let trust_text = concat!(
            "Accessing workspace:\n",
            "/workspace/AgentHouse\n",
            "Quick safety check: Is this a project you created or one you trust?\n",
            "1. Yes, I trust this folder\n",
        );
        assert!(text_looks_like_claude_trust_prompt(trust_text));

        let prompt_text = "Claude Code\nType a message or /help";
        assert!(!text_looks_like_claude_trust_prompt(prompt_text));
    }

    #[test]
    fn claude_tui_marker_prompt_does_not_include_exact_marker() {
        let marker = "AH_CLAUDE_TUI_PROMPT_20260603";
        let prompt = claude_tui_marker_prompt(marker);

        assert!(!prompt.contains(marker));
        assert!(prompt.contains("AH_CLAUDE_TUI_PROMPT"));
        assert!(prompt.contains("20260603"));
    }
}
