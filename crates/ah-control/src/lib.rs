use std::path::PathBuf;

use ah_block::{Block, BlockAttachment};
use ah_core::{SessionId, TabId, WindowId, WorkspaceId};
use ah_web::{
    BrowserAction, BrowserActionResult, BrowserInput, BrowserSessionState, BrowserSurfaceSnapshot,
    ViewportSize,
};
use ah_workspace::{LayoutMode, WindowTab, Workspace, WorkspaceWindow};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_SOCKET_NAME: &str = "agenthouse.sock";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlRequest {
    pub id: String,
    pub command: ControlCommand,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlCommand {
    Ping,
    Snapshot,
    GetAppSettings,
    SetUiLanguage {
        language: UiLanguagePreference,
    },
    SetUiThemeScheme {
        scheme: UiThemeSchemePreference,
    },
    ListEvents {
        since_sequence: Option<u64>,
        limit: Option<usize>,
    },
    WatchEvents {
        since_sequence: Option<u64>,
        limit: Option<usize>,
    },
    CaptureSurface {
        window_id: Option<WindowId>,
    },
    CaptureSessionSurface {
        session_id: SessionId,
    },
    CreateWorkspace {
        name: String,
        root: Option<PathBuf>,
    },
    ListWorkspaces,
    ActivateWorkspace {
        workspace_id: WorkspaceId,
    },
    ListWindows {
        workspace_id: Option<WorkspaceId>,
    },
    CreateWindow {
        workspace_id: WorkspaceId,
        title: String,
    },
    ListWindowTabs {
        window_id: WindowId,
    },
    ListSessions {
        workspace_id: Option<WorkspaceId>,
    },
    ActivateWindow {
        workspace_id: WorkspaceId,
        window_id: WindowId,
    },
    ActivateWindowTab {
        window_id: WindowId,
        tab_id: TabId,
    },
    CloseWindow {
        window_id: WindowId,
    },
    CloseWindowTab {
        window_id: WindowId,
        tab_id: TabId,
    },
    MoveWindowTab {
        source_window_id: WindowId,
        tab_id: TabId,
        target_window_id: WindowId,
    },
    SetWorkspaceLayout {
        workspace_id: WorkspaceId,
        mode: LayoutMode,
    },
    OpenTerminalWindow {
        workspace_id: WorkspaceId,
        title: String,
        cwd: Option<PathBuf>,
    },
    OpenTerminalTab {
        window_id: WindowId,
        title: String,
        cwd: Option<PathBuf>,
    },
    OpenWebWindow {
        workspace_id: WorkspaceId,
        title: String,
        url: String,
    },
    OpenWebTab {
        window_id: WindowId,
        title: String,
        url: String,
    },
    SplitWindow {
        window_id: WindowId,
        direction: WindowSplitDirection,
    },
    ListBrowserSessions {
        workspace_id: Option<WorkspaceId>,
    },
    GetBrowserSession {
        session_id: SessionId,
    },
    CaptureBrowserSurface {
        session_id: SessionId,
    },
    BrowserNavigate {
        session_id: SessionId,
        url: String,
    },
    BrowserAction {
        session_id: SessionId,
        action: BrowserAction,
    },
    SendBrowserInput {
        session_id: SessionId,
        input: BrowserInput,
    },
    ResizeBrowser {
        session_id: SessionId,
        viewport: ViewportSize,
    },
    RunTerminalCommand {
        session_id: SessionId,
        command: String,
    },
    WriteTerminalInput {
        session_id: SessionId,
        input: String,
    },
    SendTerminalKey {
        session_id: SessionId,
        key: TerminalKeyInput,
    },
    InterruptSession {
        session_id: SessionId,
    },
    TerminateSession {
        session_id: SessionId,
    },
    RestartSession {
        session_id: SessionId,
    },
    ResizeTerminal {
        session_id: SessionId,
        cols: u16,
        rows: u16,
    },
    GetSession {
        session_id: SessionId,
    },
    AckSessionRing {
        session_id: SessionId,
    },
    ListBlocks {
        session_id: SessionId,
    },
    ForwardBlock {
        source_session_id: SessionId,
        block_id: ah_core::BlockId,
        target_session_id: SessionId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlResponse {
    pub id: String,
    pub result: ControlResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResult {
    Pong {
        protocol: String,
    },
    Snapshot(ControlSnapshot),
    AppSettings {
        settings: AppSettingsSummary,
    },
    UiLanguageSet {
        settings: AppSettingsSummary,
    },
    UiThemeSchemeSet {
        settings: AppSettingsSummary,
    },
    Events {
        events: Vec<ControlEvent>,
    },
    SurfaceCapture(Box<SurfaceCapture>),
    WorkspaceCreated {
        workspace: WorkspaceSummary,
    },
    Workspaces {
        workspaces: Vec<WorkspaceSummary>,
    },
    Windows {
        windows: Vec<WindowSummary>,
    },
    WindowCreated {
        workspace_id: WorkspaceId,
        window: WindowSummary,
    },
    WindowTabs {
        window_id: WindowId,
        tabs: Vec<WindowTabSummary>,
    },
    Sessions {
        sessions: Vec<SessionSummary>,
    },
    WorkspaceActivated {
        workspace_id: WorkspaceId,
    },
    WindowActivated {
        workspace_id: WorkspaceId,
        window_id: WindowId,
    },
    WindowTabActivated {
        window_id: WindowId,
        tab_id: TabId,
    },
    WindowClosed {
        workspace_id: WorkspaceId,
        window_id: WindowId,
    },
    WindowTabClosed {
        window_id: WindowId,
        tab_id: TabId,
    },
    WindowTabMoved {
        tab_id: TabId,
        source_window_id: WindowId,
        target_window_id: WindowId,
    },
    WorkspaceLayoutSet {
        workspace_id: WorkspaceId,
        mode: LayoutMode,
    },
    WindowOpened {
        workspace_id: WorkspaceId,
        window: WindowSummary,
    },
    WindowTabOpened {
        window_id: WindowId,
        tab: WindowTabSummary,
    },
    WindowSplit {
        workspace_id: WorkspaceId,
        source_window_id: WindowId,
        window: WindowSummary,
    },
    CommandQueued {
        session_id: SessionId,
    },
    TerminalInputWritten {
        session_id: SessionId,
    },
    TerminalKeySent {
        session_id: SessionId,
    },
    SessionInterrupted {
        session: SessionSummary,
    },
    SessionTerminated {
        session: SessionSummary,
    },
    SessionRestarted {
        session: SessionSummary,
    },
    TerminalResized {
        session_id: SessionId,
        cols: u16,
        rows: u16,
    },
    Session {
        session: SessionSummary,
    },
    SessionRingAcknowledged {
        session: SessionSummary,
    },
    Blocks {
        session_id: SessionId,
        blocks: Vec<BlockSummary>,
    },
    BlockForwarded {
        source_session_id: SessionId,
        target_session_id: SessionId,
        block: BlockSummary,
    },
    BrowserSessions {
        sessions: Vec<BrowserSessionSummary>,
    },
    BrowserSession {
        session: BrowserSessionSummary,
    },
    BrowserSurface {
        surface: BrowserSurfaceSnapshot,
    },
    BrowserActionApplied {
        result: BrowserActionResult,
    },
    BrowserResized {
        session_id: SessionId,
        viewport: ViewportSize,
    },
    Error(ControlErrorInfo),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowSplitDirection {
    Right,
    Down,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlStreamMessage {
    Event(ControlEvent),
    Heartbeat { last_sequence: u64 },
    Error(ControlErrorInfo),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistedControlState {
    pub active_workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    pub ui_language: Option<UiLanguagePreference>,
    #[serde(default)]
    pub ui_theme_scheme: Option<String>,
    #[serde(default)]
    pub ui_theme_mode: Option<String>,
    pub next_event_sequence: u64,
    pub events: Vec<ControlEvent>,
    pub session_rings: Vec<PersistedSessionRing>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguagePreference {
    ZhCn,
    En,
}

impl UiLanguagePreference {
    pub const ALL: [Self; 2] = [Self::ZhCn, Self::En];

    #[must_use]
    pub fn all() -> &'static [Self] {
        &Self::ALL
    }

    #[must_use]
    pub fn control_code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-cn",
            Self::En => "en",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ZhCn => "Chinese",
            Self::En => "English",
        }
    }

    #[must_use]
    pub fn native_label(self) -> &'static str {
        match self {
            Self::ZhCn => "中文",
            Self::En => "English",
        }
    }

    #[must_use]
    pub fn from_control_code(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh_cn" | "zh-hans" | "cn" | "chinese" | "中文" => Some(Self::ZhCn),
            "en" | "en-us" | "en_us" | "english" => Some(Self::En),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiThemeSchemePreference {
    Cream,
    Warm,
    Blue,
    Green,
    Red,
    Purple,
    Glass,
    Luxury,
    Soft,
}

impl UiThemeSchemePreference {
    pub const ALL: [Self; 9] = [
        Self::Cream,
        Self::Warm,
        Self::Blue,
        Self::Green,
        Self::Red,
        Self::Purple,
        Self::Glass,
        Self::Luxury,
        Self::Soft,
    ];

    #[must_use]
    pub fn all() -> &'static [Self] {
        &Self::ALL
    }

    #[must_use]
    pub fn control_code(self) -> &'static str {
        match self {
            Self::Cream => "cream",
            Self::Warm => "warm",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Red => "red",
            Self::Purple => "purple",
            Self::Glass => "glass",
            Self::Luxury => "luxury",
            Self::Soft => "soft",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cream => "Xuan Paper",
            Self::Warm => "Warm",
            Self::Blue => "Blue",
            Self::Green => "Green",
            Self::Red => "Red",
            Self::Purple => "Purple",
            Self::Glass => "Glass Magazine",
            Self::Luxury => "Luxury",
            Self::Soft => "Soft",
        }
    }

    #[must_use]
    pub fn native_label(self) -> &'static str {
        match self {
            Self::Cream => "宣纸",
            Self::Warm => "暖黄",
            Self::Blue => "蓝",
            Self::Green => "绿",
            Self::Red => "红",
            Self::Purple => "紫",
            Self::Glass => "杂志",
            Self::Luxury => "奢华",
            Self::Soft => "柔",
        }
    }

    #[must_use]
    pub fn from_control_code(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cream" | "xuan" | "xuan-paper" | "xuan_paper" | "宣纸" => Some(Self::Cream),
            "warm" | "yellow" | "暖黄" => Some(Self::Warm),
            "blue" | "蓝" => Some(Self::Blue),
            "green" | "绿" => Some(Self::Green),
            "red" | "红" => Some(Self::Red),
            "purple" | "紫" => Some(Self::Purple),
            "glass" | "magazine" | "editorial" | "杂志" => Some(Self::Glass),
            "luxury" | "奢华" => Some(Self::Luxury),
            "soft" | "柔" => Some(Self::Soft),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UiLanguageOptionSummary {
    pub preference: UiLanguagePreference,
    pub code: String,
    pub label: String,
    pub native_label: String,
}

impl UiLanguageOptionSummary {
    #[must_use]
    pub fn from_preference(preference: UiLanguagePreference) -> Self {
        Self {
            preference,
            code: preference.control_code().to_string(),
            label: preference.label().to_string(),
            native_label: preference.native_label().to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UiThemeSchemeOptionSummary {
    pub preference: UiThemeSchemePreference,
    pub code: String,
    pub label: String,
    pub native_label: String,
}

impl UiThemeSchemeOptionSummary {
    #[must_use]
    pub fn from_preference(preference: UiThemeSchemePreference) -> Self {
        Self {
            preference,
            code: preference.control_code().to_string(),
            label: preference.label().to_string(),
            native_label: preference.native_label().to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppSettingsSummary {
    pub ui_language: UiLanguagePreference,
    pub ui_language_options: Vec<UiLanguageOptionSummary>,
    pub ui_theme_scheme: String,
    pub ui_theme_scheme_options: Vec<UiThemeSchemeOptionSummary>,
    pub ui_theme_mode: String,
}

impl AppSettingsSummary {
    #[must_use]
    pub fn new(
        ui_language: UiLanguagePreference,
        ui_theme_scheme: UiThemeSchemePreference,
        ui_theme_mode: impl Into<String>,
    ) -> Self {
        Self {
            ui_language,
            ui_language_options: UiLanguagePreference::all()
                .iter()
                .copied()
                .map(UiLanguageOptionSummary::from_preference)
                .collect(),
            ui_theme_scheme: ui_theme_scheme.control_code().to_string(),
            ui_theme_scheme_options: UiThemeSchemePreference::all()
                .iter()
                .copied()
                .map(UiThemeSchemeOptionSummary::from_preference)
                .collect(),
            ui_theme_mode: ui_theme_mode.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistedSessionRing {
    pub session_id: SessionId,
    pub state: String,
    pub summary: String,
    pub unread_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalKeyInput {
    pub key: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub modifiers: TerminalKeyInputModifiers,
}

impl TerminalKeyInput {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: None,
            modifiers: TerminalKeyInputModifiers::default(),
        }
    }

    #[must_use]
    pub fn text(key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: Some(text.into()),
            modifiers: TerminalKeyInputModifiers::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalKeyInputModifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub platform: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlSnapshot {
    pub active_workspace_id: Option<WorkspaceId>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub windows: Vec<WindowSummary>,
    pub sessions: Vec<SessionSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlEvent {
    pub sequence: u64,
    pub level: String,
    pub topic: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceCapture {
    pub mode: String,
    pub active_workspace_id: Option<WorkspaceId>,
    pub active_window_id: Option<WindowId>,
    pub target_window_id: Option<WindowId>,
    pub workspace_name: Option<String>,
    pub window_title: Option<String>,
    pub content_type: Option<String>,
    pub target_url: Option<String>,
    pub target_path: Option<PathBuf>,
    pub session: Option<SessionSummary>,
    pub recent_blocks: Vec<BlockSummary>,
    pub terminal_tail: Option<String>,
    pub screenshot_path: Option<PathBuf>,
    pub snapshot_path: Option<PathBuf>,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSummary {
    pub id: WorkspaceId,
    pub name: String,
    pub root: Option<PathBuf>,
    pub active: bool,
    pub window_count: usize,
    pub layout_mode: LayoutMode,
}

impl WorkspaceSummary {
    #[must_use]
    pub fn from_workspace(workspace: &Workspace, active: bool) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name.clone(),
            root: workspace.root.clone(),
            active,
            window_count: workspace.windows.len(),
            layout_mode: workspace.layout.mode.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WindowSummary {
    pub id: WindowId,
    pub workspace_id: WorkspaceId,
    pub title: String,
    pub content_type: String,
    pub session_id: Option<SessionId>,
    pub active: bool,
    pub active_tab_id: Option<TabId>,
    pub tab_count: usize,
}

impl WindowSummary {
    #[must_use]
    pub fn from_window(workspace: &Workspace, window: &WorkspaceWindow) -> Self {
        let (content_type, session_id) = window
            .active_tab()
            .map(active_tab_content_summary)
            .unwrap_or_else(|| ("empty".to_string(), None));

        Self {
            id: window.id,
            workspace_id: workspace.id,
            title: window.title.clone(),
            content_type,
            session_id,
            active: workspace.active_window_id == Some(window.id),
            active_tab_id: window.active_tab().map(|tab| tab.id),
            tab_count: window.tabs.len(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WindowTabSummary {
    pub id: TabId,
    pub window_id: WindowId,
    pub title: String,
    pub content_type: String,
    pub session_id: Option<SessionId>,
    pub active: bool,
}

impl WindowTabSummary {
    #[must_use]
    pub fn from_tab(window: &WorkspaceWindow, tab: &WindowTab) -> Self {
        let (content_type, session_id) = active_tab_content_summary(tab);

        Self {
            id: tab.id,
            window_id: window.id,
            title: tab.title.clone(),
            content_type,
            session_id,
            active: window.active_tab_id == Some(tab.id),
        }
    }
}

fn active_tab_content_summary(tab: &WindowTab) -> (String, Option<SessionId>) {
    match &tab.content {
        ah_workspace::WindowContent::Terminal { session_id } => {
            ("terminal".to_string(), Some(*session_id))
        }
        ah_workspace::WindowContent::Web { session_id, .. } => {
            ("web".to_string(), Some(*session_id))
        }
        ah_workspace::WindowContent::FilePreview { .. } => ("file_preview".to_string(), None),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSessionSummary {
    pub id: SessionId,
    pub title: String,
    pub current_url: String,
    pub engine: String,
    pub status: String,
    pub viewport: ViewportSize,
    pub last_error: Option<String>,
}

impl BrowserSessionSummary {
    #[must_use]
    pub fn from_state(state: &BrowserSessionState) -> Self {
        Self {
            id: state.id,
            title: state.title.clone(),
            current_url: state.current_url.clone(),
            engine: format!("{:?}", state.engine),
            status: format!("{:?}", state.status),
            viewport: state.viewport,
            last_error: state.last_error.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub name: String,
    pub status: String,
    pub block_count: usize,
    pub ring_state: String,
    pub ring_summary: String,
    pub unread_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockSummary {
    pub id: ah_core::BlockId,
    pub session_id: SessionId,
    pub kind: String,
    pub state: String,
    pub text: String,
    pub attachments: Vec<BlockAttachmentSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockAttachmentSummary {
    pub kind: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    pub media_type: Option<String>,
    pub byte_count: Option<usize>,
}

impl BlockSummary {
    #[must_use]
    pub fn from_block(block: &Block) -> Self {
        Self {
            id: block.id,
            session_id: block.session_id,
            kind: format!("{:?}", block.kind),
            state: format!("{:?}", block.state),
            text: block.text.clone(),
            attachments: block
                .attachments
                .iter()
                .map(BlockAttachmentSummary::from_attachment)
                .collect(),
        }
    }
}

impl BlockAttachmentSummary {
    #[must_use]
    pub fn from_attachment(attachment: &BlockAttachment) -> Self {
        match attachment {
            BlockAttachment::File { path } => Self {
                kind: "file".to_string(),
                path: Some(path.clone()),
                url: None,
                media_type: None,
                byte_count: None,
            },
            BlockAttachment::Url { url } => Self {
                kind: "url".to_string(),
                path: None,
                url: Some(url.clone()),
                media_type: None,
                byte_count: None,
            },
            BlockAttachment::Image { media_type, bytes } => Self {
                kind: "image".to_string(),
                path: None,
                url: None,
                media_type: Some(media_type.clone()),
                byte_count: Some(bytes.len()),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControlErrorInfo {
    pub code: String,
    pub message: String,
}

impl ControlErrorInfo {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ControlProtocolError {
    #[error("failed to decode request: {0}")]
    Decode(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettingsSummary, BlockSummary, BrowserSessionSummary, ControlCommand, ControlRequest,
        ControlResponse, ControlResult, TerminalKeyInput, TerminalKeyInputModifiers,
        UiThemeSchemePreference, WindowSplitDirection, WindowTabSummary,
    };
    use ah_block::{Block, BlockAttachment, BlockKind};
    use ah_core::{Actor, SessionId, WindowId, WorkspaceId};
    use ah_web::{BrowserAction, BrowserEngine, BrowserInput, BrowserSessionState, ViewportSize};
    use ah_workspace::{LayoutMode, WindowTab, WorkspaceWindow};
    use std::path::PathBuf;

    #[test]
    fn decodes_json_line_request() {
        let request: ControlRequest =
            serde_json::from_str(r#"{"id":"1","command":{"type":"ping"}}"#)
                .expect("ping request json should decode");

        assert_eq!(request.id, "1");
        assert_eq!(request.command, ControlCommand::Ping);
    }

    #[test]
    fn encodes_json_line_response() {
        let response = ControlResponse {
            id: "1".to_string(),
            result: ControlResult::Pong {
                protocol: "agenthouse-control/0.1".to_string(),
            },
        };

        let json = serde_json::to_string(&response).expect("pong response should encode");

        assert_eq!(
            json,
            r#"{"id":"1","result":{"type":"pong","protocol":"agenthouse-control/0.1"}}"#
        );
    }

    #[test]
    fn encodes_set_workspace_layout_request() {
        let workspace_id = WorkspaceId::new();
        let request = ControlRequest {
            id: "layout".to_string(),
            command: ControlCommand::SetWorkspaceLayout {
                workspace_id,
                mode: LayoutMode::Grid,
            },
        };

        let json = serde_json::to_value(&request).expect("request should encode");

        assert_eq!(json["command"]["type"], "set_workspace_layout");
        assert_eq!(
            json["command"]["workspace_id"],
            serde_json::json!(workspace_id)
        );
        assert_eq!(json["command"]["mode"], "grid");
    }

    #[test]
    fn encodes_ui_language_setting_request() {
        let request = ControlRequest {
            id: "language".to_string(),
            command: ControlCommand::SetUiLanguage {
                language: super::UiLanguagePreference::En,
            },
        };

        let json = serde_json::to_value(request).expect("request should encode");

        assert_eq!(json["command"]["type"], "set_ui_language");
        assert_eq!(json["command"]["language"], "en");
    }

    #[test]
    fn encodes_ui_theme_scheme_setting_request() {
        let request = ControlRequest {
            id: "theme".to_string(),
            command: ControlCommand::SetUiThemeScheme {
                scheme: UiThemeSchemePreference::Glass,
            },
        };

        let json = serde_json::to_value(request).expect("request should encode");

        assert_eq!(json["command"]["type"], "set_ui_theme_scheme");
        assert_eq!(json["command"]["scheme"], "glass");
    }

    #[test]
    fn app_settings_summary_exposes_language_control_options() {
        let summary = AppSettingsSummary::new(
            super::UiLanguagePreference::ZhCn,
            UiThemeSchemePreference::Glass,
            "light",
        );

        assert_eq!(summary.ui_language, super::UiLanguagePreference::ZhCn);
        assert_eq!(summary.ui_language_options.len(), 2);
        assert_eq!(
            summary.ui_language_options[0].preference,
            super::UiLanguagePreference::ZhCn
        );
        assert_eq!(summary.ui_language_options[0].code, "zh-cn");
        assert_eq!(summary.ui_language_options[0].native_label, "中文");
        assert_eq!(
            summary.ui_language_options[1].preference,
            super::UiLanguagePreference::En
        );
        assert_eq!(summary.ui_language_options[1].code, "en");
        assert_eq!(summary.ui_language_options[1].native_label, "English");
        assert_eq!(summary.ui_theme_scheme, "glass");
        assert_eq!(summary.ui_theme_scheme_options.len(), 9);
        assert_eq!(summary.ui_theme_scheme_options[0].code, "cream");
        assert_eq!(summary.ui_theme_scheme_options[6].code, "glass");
        assert_eq!(summary.ui_theme_scheme_options[6].native_label, "杂志");
        assert_eq!(summary.ui_theme_mode, "light");

        let json = serde_json::to_value(summary).expect("settings should encode");

        assert_eq!(json["ui_language"], "zh_cn");
        assert_eq!(json["ui_language_options"][0]["code"], "zh-cn");
        assert_eq!(json["ui_language_options"][1]["code"], "en");
        assert_eq!(json["ui_theme_scheme"], "glass");
        assert_eq!(json["ui_theme_scheme_options"][6]["code"], "glass");
        assert_eq!(json["ui_theme_scheme_options"][6]["native_label"], "杂志");
        assert_eq!(json["ui_theme_mode"], "light");
    }

    #[test]
    fn decodes_persisted_control_state_without_ui_settings() {
        let json = r#"{
            "active_workspace_id": null,
            "next_event_sequence": 1,
            "events": [],
            "session_rings": []
        }"#;

        let state: super::PersistedControlState =
            serde_json::from_str(json).expect("old control state should decode");

        assert_eq!(state.ui_language, None);
        assert_eq!(state.ui_theme_scheme, None);
        assert_eq!(state.ui_theme_mode, None);
    }

    #[test]
    fn encodes_targeted_window_tab_requests() {
        let window_id = WindowId::new();
        let terminal = ControlRequest {
            id: "terminal-tab".to_string(),
            command: ControlCommand::OpenTerminalTab {
                window_id,
                title: "Shell".to_string(),
                cwd: Some(PathBuf::from("/tmp/agenthouse")),
            },
        };
        let web = ControlRequest {
            id: "web-tab".to_string(),
            command: ControlCommand::OpenWebTab {
                window_id,
                title: "Web".to_string(),
                url: "https://example.com".to_string(),
            },
        };
        let split = ControlRequest {
            id: "split".to_string(),
            command: ControlCommand::SplitWindow {
                window_id,
                direction: WindowSplitDirection::Right,
            },
        };

        let terminal_json = serde_json::to_value(terminal).expect("request should encode");
        let web_json = serde_json::to_value(web).expect("request should encode");
        let split_json = serde_json::to_value(split).expect("request should encode");

        assert_eq!(terminal_json["command"]["type"], "open_terminal_tab");
        assert_eq!(
            terminal_json["command"]["window_id"],
            serde_json::json!(window_id)
        );
        assert_eq!(terminal_json["command"]["cwd"], "/tmp/agenthouse");
        assert_eq!(web_json["command"]["type"], "open_web_tab");
        assert_eq!(web_json["command"]["url"], "https://example.com");
        assert_eq!(split_json["command"]["type"], "split_window");
        assert_eq!(split_json["command"]["direction"], "right");
    }

    #[test]
    fn encodes_session_lifecycle_requests() {
        let session_id = ah_core::SessionId::new();
        let input = ControlRequest {
            id: "write".to_string(),
            command: ControlCommand::WriteTerminalInput {
                session_id,
                input: "echo ok\n".to_string(),
            },
        };
        let interrupt = ControlRequest {
            id: "interrupt".to_string(),
            command: ControlCommand::InterruptSession { session_id },
        };
        let resize = ControlRequest {
            id: "resize".to_string(),
            command: ControlCommand::ResizeTerminal {
                session_id,
                cols: 120,
                rows: 40,
            },
        };
        let key = ControlRequest {
            id: "key".to_string(),
            command: ControlCommand::SendTerminalKey {
                session_id,
                key: TerminalKeyInput {
                    key: "up".to_string(),
                    text: None,
                    modifiers: TerminalKeyInputModifiers {
                        control: true,
                        ..TerminalKeyInputModifiers::default()
                    },
                },
            },
        };

        let input_json = serde_json::to_value(input).expect("request should encode");
        let interrupt_json = serde_json::to_value(interrupt).expect("request should encode");
        let resize_json = serde_json::to_value(resize).expect("request should encode");
        let key_json = serde_json::to_value(key).expect("request should encode");

        assert_eq!(input_json["command"]["type"], "write_terminal_input");
        assert_eq!(
            input_json["command"]["session_id"],
            serde_json::json!(session_id)
        );
        assert_eq!(interrupt_json["command"]["type"], "interrupt_session");
        assert_eq!(resize_json["command"]["type"], "resize_terminal");
        assert_eq!(resize_json["command"]["cols"], 120);
        assert_eq!(resize_json["command"]["rows"], 40);
        assert_eq!(key_json["command"]["type"], "send_terminal_key");
        assert_eq!(key_json["command"]["key"]["key"], "up");
        assert_eq!(key_json["command"]["key"]["modifiers"]["control"], true);
    }

    #[test]
    fn encodes_session_surface_request() {
        let session_id = ah_core::SessionId::new();
        let request = ControlRequest {
            id: "surface".to_string(),
            command: ControlCommand::CaptureSessionSurface { session_id },
        };

        let json = serde_json::to_value(request).expect("request should encode");

        assert_eq!(json["command"]["type"], "capture_session_surface");
        assert_eq!(json["command"]["session_id"], serde_json::json!(session_id));
    }

    #[test]
    fn encodes_browser_control_requests() {
        let session_id = SessionId::new();
        let navigate = ControlRequest {
            id: "browser-nav".to_string(),
            command: ControlCommand::BrowserNavigate {
                session_id,
                url: "https://example.com".to_string(),
            },
        };
        let action = ControlRequest {
            id: "browser-action".to_string(),
            command: ControlCommand::BrowserAction {
                session_id,
                action: BrowserAction::Click {
                    selector: "button".to_string(),
                },
            },
        };
        let resize = ControlRequest {
            id: "browser-resize".to_string(),
            command: ControlCommand::ResizeBrowser {
                session_id,
                viewport: ViewportSize {
                    width: 1024,
                    height: 768,
                },
            },
        };
        let input = ControlRequest {
            id: "browser-input".to_string(),
            command: ControlCommand::SendBrowserInput {
                session_id,
                input: BrowserInput::KeyText {
                    text: "hello".to_string(),
                },
            },
        };

        let navigate_json = serde_json::to_value(navigate).expect("request should encode");
        let action_json = serde_json::to_value(action).expect("request should encode");
        let resize_json = serde_json::to_value(resize).expect("request should encode");
        let input_json = serde_json::to_value(input).expect("request should encode");

        assert_eq!(navigate_json["command"]["type"], "browser_navigate");
        assert_eq!(
            navigate_json["command"]["session_id"],
            serde_json::json!(session_id)
        );
        assert_eq!(action_json["command"]["type"], "browser_action");
        assert_eq!(action_json["command"]["action"]["type"], "click");
        assert_eq!(resize_json["command"]["type"], "resize_browser");
        assert_eq!(resize_json["command"]["viewport"]["width"], 1024);
        assert_eq!(input_json["command"]["type"], "send_browser_input");
        assert_eq!(input_json["command"]["input"]["KeyText"]["text"], "hello");
    }

    #[test]
    fn web_tab_summary_exposes_browser_session_id() {
        let session_id = SessionId::new();
        let mut window = WorkspaceWindow::new("Browser Window");
        let tab = WindowTab::web_with_session("Browser", session_id, "https://example.com");
        let tab_id = tab.id;
        window.push_tab(tab);

        let summary =
            WindowTabSummary::from_tab(&window, window.active_tab().expect("tab should be active"));

        assert_eq!(summary.id, tab_id);
        assert_eq!(summary.content_type, "web");
        assert_eq!(summary.session_id, Some(session_id));
    }

    #[test]
    fn browser_session_summary_is_stable_control_shape() {
        let state = BrowserSessionState::new("Browser", "about:blank", BrowserEngine::TextPreview);

        let summary = BrowserSessionSummary::from_state(&state);

        assert_eq!(summary.id, state.id);
        assert_eq!(summary.title, "Browser");
        assert_eq!(summary.current_url, "about:blank");
        assert_eq!(summary.engine, "TextPreview");
        assert_eq!(summary.status, "Idle");
    }

    #[test]
    fn encodes_watch_events_request() {
        let request = ControlRequest {
            id: "watch".to_string(),
            command: ControlCommand::WatchEvents {
                since_sequence: Some(7),
                limit: Some(3),
            },
        };

        let json = serde_json::to_value(request).expect("request should encode");

        assert_eq!(json["command"]["type"], "watch_events");
        assert_eq!(json["command"]["since_sequence"], 7);
        assert_eq!(json["command"]["limit"], 3);
    }

    #[test]
    fn block_summary_includes_attachment_metadata() {
        let mut block = Block::new(
            SessionId::new(),
            Actor::Agent {
                name: "test-agent".to_string(),
            },
            BlockKind::AgentOutput,
            "result",
        );
        block.attach(BlockAttachment::File {
            path: PathBuf::from("/tmp/raw.jsonl"),
        });
        block.attach(BlockAttachment::Url {
            url: "https://example.com".to_string(),
        });
        block.attach(BlockAttachment::Image {
            media_type: "image/png".to_string(),
            bytes: vec![1, 2, 3],
        });

        let summary = BlockSummary::from_block(&block);

        assert_eq!(summary.attachments.len(), 3);
        assert_eq!(summary.attachments[0].kind, "file");
        assert_eq!(
            summary.attachments[0].path,
            Some(PathBuf::from("/tmp/raw.jsonl"))
        );
        assert_eq!(summary.attachments[1].kind, "url");
        assert_eq!(
            summary.attachments[1].url.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(summary.attachments[2].kind, "image");
        assert_eq!(
            summary.attachments[2].media_type.as_deref(),
            Some("image/png")
        );
        assert_eq!(summary.attachments[2].byte_count, Some(3));
    }
}
