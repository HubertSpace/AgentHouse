use std::io::Read;
use std::time::Duration;

use ah_core::{SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_TEXT_FETCH_LIMIT_BYTES: usize = 64 * 1024;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

impl Default for ViewportSize {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PageSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub status: Option<u16>,
    pub byte_count: Option<usize>,
    pub truncated: bool,
    pub captured_at: Timestamp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebTextSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub status: Option<u16>,
    pub byte_count: Option<usize>,
    pub truncated: bool,
    pub captured_at: Timestamp,
}

impl From<PageSnapshot> for WebTextSnapshot {
    fn from(snapshot: PageSnapshot) -> Self {
        Self {
            url: snapshot.url,
            title: snapshot.title,
            text: snapshot.text,
            status: snapshot.status,
            byte_count: snapshot.byte_count,
            truncated: snapshot.truncated,
            captured_at: snapshot.captured_at,
        }
    }
}

impl From<WebTextSnapshot> for PageSnapshot {
    fn from(snapshot: WebTextSnapshot) -> Self {
        Self {
            url: snapshot.url,
            title: snapshot.title,
            text: snapshot.text,
            status: snapshot.status,
            byte_count: snapshot.byte_count,
            truncated: snapshot.truncated,
            captured_at: snapshot.captured_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserEngine {
    TextPreview,
    Native,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserLoadStatus {
    Idle,
    Loading,
    Loaded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSessionState {
    pub id: SessionId,
    pub title: String,
    pub current_url: String,
    pub engine: BrowserEngine,
    pub status: BrowserLoadStatus,
    pub viewport: ViewportSize,
    pub last_snapshot: Option<PageSnapshot>,
    pub last_error: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl BrowserSessionState {
    #[must_use]
    pub fn new(title: impl Into<String>, url: impl Into<String>, engine: BrowserEngine) -> Self {
        let now = Timestamp::now();
        Self {
            id: SessionId::new(),
            title: title.into(),
            current_url: url.into(),
            engine,
            status: BrowserLoadStatus::Idle,
            viewport: ViewportSize::default(),
            last_snapshot: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn with_id(
        id: SessionId,
        title: impl Into<String>,
        url: impl Into<String>,
        engine: BrowserEngine,
    ) -> Self {
        let mut state = Self::new(title, url, engine);
        state.id = id;
        state
    }

    pub fn mark_loading(&mut self) {
        self.status = BrowserLoadStatus::Loading;
        self.last_error = None;
        self.updated_at = Timestamp::now();
    }

    pub fn navigate_to(&mut self, url: impl Into<String>) {
        self.current_url = url.into();
        self.mark_loading();
    }

    pub fn resize(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        self.updated_at = Timestamp::now();
    }

    pub fn apply_snapshot(&mut self, snapshot: PageSnapshot) {
        self.current_url = snapshot.url.clone();
        if let Some(title) = snapshot.title.as_ref().filter(|title| !title.is_empty()) {
            self.title = title.clone();
        }
        self.status = BrowserLoadStatus::Loaded;
        self.last_error = None;
        self.updated_at = snapshot.captured_at;
        self.last_snapshot = Some(snapshot);
    }

    pub fn apply_error(&mut self, error: impl Into<String>) {
        self.status = BrowserLoadStatus::Failed;
        self.last_error = Some(error.into());
        self.updated_at = Timestamp::now();
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserSurfaceSnapshot {
    pub session: BrowserSessionState,
    pub page: Option<PageSnapshot>,
    pub frame: Option<BrowserFrameSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserFrameSnapshot {
    pub format: BrowserFrameFormat,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub byte_count: usize,
    pub captured_at: Timestamp,
}

impl BrowserFrameSnapshot {
    #[must_use]
    pub fn png(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self::new(BrowserFrameFormat::Png, width, height, bytes)
    }

    #[must_use]
    pub fn rgba8(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self::new(BrowserFrameFormat::Rgba8, width, height, bytes)
    }

    #[must_use]
    pub fn new(format: BrowserFrameFormat, width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self {
            format,
            width,
            height,
            byte_count: bytes.len(),
            bytes,
            captured_at: Timestamp::now(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFrameFormat {
    Rgba8,
    Png,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserBackendSnapshot {
    pub page: Option<PageSnapshot>,
    pub frame: Option<BrowserFrameSnapshot>,
}

impl BrowserBackendSnapshot {
    #[must_use]
    pub fn from_page(page: PageSnapshot) -> Self {
        Self {
            page: Some(page),
            frame: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BrowserActionResult {
    pub session: BrowserSessionState,
    pub message: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate {
        url: String,
    },
    Reload,
    Back,
    Forward,
    Click {
        selector: String,
    },
    Fill {
        selector: String,
        value: String,
    },
    Type {
        selector: String,
        text: String,
    },
    PressKey {
        key: String,
        #[serde(default)]
        selector: Option<String>,
    },
    SelectOption {
        selector: String,
        value: String,
    },
    Evaluate {
        expression: String,
    },
    Snapshot,
}

impl BrowserAction {
    #[must_use]
    pub fn navigate(url: impl Into<String>) -> Self {
        Self::Navigate { url: url.into() }
    }

    #[must_use]
    pub fn requires_control_backend(&self) -> bool {
        matches!(
            self,
            Self::Click { .. }
                | Self::Fill { .. }
                | Self::Type { .. }
                | Self::PressKey { .. }
                | Self::SelectOption { .. }
                | Self::Evaluate { .. }
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BrowserInput {
    MouseClick {
        x: i32,
        y: i32,
    },
    MouseScroll {
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
    },
    KeyText {
        text: String,
    },
    KeyPress {
        key: String,
    },
    Back,
    Forward,
    Reload,
}

pub trait BrowserBackend: std::fmt::Debug {
    fn engine(&self) -> BrowserEngine;
    fn open(&mut self, url: &str) -> Result<(), WebError>;
    fn navigate(&mut self, url: &str) -> Result<(), WebError>;
    fn reload(&mut self) -> Result<(), WebError>;
    fn go_back(&mut self) -> Result<(), WebError>;
    fn go_forward(&mut self) -> Result<(), WebError>;
    fn resize(&mut self, size: ViewportSize) -> Result<(), WebError>;
    fn input(&mut self, input: BrowserInput) -> Result<(), WebError>;
    fn action(&mut self, action: &BrowserAction) -> Result<Option<String>, WebError> {
        match action {
            BrowserAction::Navigate { url } => {
                self.navigate(url)?;
                Ok(None)
            }
            BrowserAction::Reload => {
                self.reload()?;
                Ok(None)
            }
            BrowserAction::Back => {
                self.go_back()?;
                Ok(None)
            }
            BrowserAction::Forward => {
                self.go_forward()?;
                Ok(None)
            }
            BrowserAction::Snapshot => Ok(None),
            action => Err(WebError::Unsupported(format!(
                "{:?} backend does not support browser action: {action:?}",
                self.engine()
            ))),
        }
    }
    fn snapshot(&mut self) -> Result<BrowserBackendSnapshot, WebError>;
}

#[derive(Debug, Error)]
pub enum WebError {
    #[error("unsupported browser backend operation: {0}")]
    Unsupported(String),
    #[error("invalid browser url: {0}")]
    InvalidUrl(String),
    #[error("browser backend failed: {0}")]
    Backend(String),
}

#[derive(Debug)]
pub struct HttpTextBrowserBackend {
    current_url: Option<String>,
    agent: ureq::Agent,
    fetch_limit_bytes: usize,
}

impl Default for HttpTextBrowserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTextBrowserBackend {
    #[must_use]
    pub fn new() -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(DEFAULT_REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            current_url: None,
            agent,
            fetch_limit_bytes: DEFAULT_TEXT_FETCH_LIMIT_BYTES,
        }
    }

    #[must_use]
    pub fn with_fetch_limit_bytes(mut self, fetch_limit_bytes: usize) -> Self {
        self.fetch_limit_bytes = fetch_limit_bytes.max(1);
        self
    }
}

impl BrowserBackend for HttpTextBrowserBackend {
    fn engine(&self) -> BrowserEngine {
        BrowserEngine::TextPreview
    }

    fn open(&mut self, url: &str) -> Result<(), WebError> {
        validate_http_url(url)?;
        self.current_url = Some(url.to_string());
        Ok(())
    }

    fn navigate(&mut self, url: &str) -> Result<(), WebError> {
        self.open(url)
    }

    fn reload(&mut self) -> Result<(), WebError> {
        match self.current_url.as_deref() {
            Some(url) => validate_http_url(url),
            None => Ok(()),
        }
    }

    fn go_back(&mut self) -> Result<(), WebError> {
        Err(WebError::Unsupported(
            "text browser backend does not maintain history".to_string(),
        ))
    }

    fn go_forward(&mut self) -> Result<(), WebError> {
        Err(WebError::Unsupported(
            "text browser backend does not maintain history".to_string(),
        ))
    }

    fn resize(&mut self, _size: ViewportSize) -> Result<(), WebError> {
        Ok(())
    }

    fn input(&mut self, input: BrowserInput) -> Result<(), WebError> {
        match input {
            BrowserInput::Back => self.go_back(),
            BrowserInput::Forward => self.go_forward(),
            BrowserInput::Reload => self.reload(),
            input => Err(WebError::Unsupported(format!(
                "text browser backend does not support input: {input:?}"
            ))),
        }
    }

    fn snapshot(&mut self) -> Result<BrowserBackendSnapshot, WebError> {
        let url = self
            .current_url
            .clone()
            .unwrap_or_else(|| "about:blank".to_string());
        if url == "about:blank" {
            return Ok(BrowserBackendSnapshot::from_page(PageSnapshot {
                url,
                title: Some("Blank".to_string()),
                text: Some(String::new()),
                status: None,
                byte_count: Some(0),
                truncated: false,
                captured_at: Timestamp::now(),
            }));
        }

        fetch_http_text_snapshot(&self.agent, &url, self.fetch_limit_bytes)
            .map(PageSnapshot::from)
            .map(BrowserBackendSnapshot::from_page)
    }
}

pub fn fetch_web_text_snapshot(url: &str) -> Result<WebTextSnapshot, WebError> {
    let mut backend = HttpTextBrowserBackend::new();
    backend.open(url)?;
    let snapshot = backend.snapshot()?;
    snapshot
        .page
        .map(Into::into)
        .ok_or_else(|| WebError::Backend("text browser did not produce a page snapshot".into()))
}

fn fetch_http_text_snapshot(
    agent: &ureq::Agent,
    url: &str,
    fetch_limit_bytes: usize,
) -> Result<WebTextSnapshot, WebError> {
    validate_http_url(url)?;
    let mut response = agent
        .get(url)
        .header("User-Agent", "AgentHouse-RS/0.1")
        .call()
        .map_err(|error| WebError::Backend(format!("failed to fetch {url}: {error}")))?;
    let status = response.status().as_u16();
    let mut raw_bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(fetch_limit_bytes.saturating_add(1) as u64)
        .read_to_end(&mut raw_bytes)
        .map_err(|error| {
            WebError::Backend(format!("failed to read response body from {url}: {error}"))
        })?;
    let byte_count = raw_bytes.len();
    let truncated = byte_count > fetch_limit_bytes;
    if truncated {
        raw_bytes.truncate(fetch_limit_bytes);
    }
    let raw = String::from_utf8_lossy(&raw_bytes).to_string();
    let extracted = extract_html_text(&raw);

    Ok(WebTextSnapshot {
        url: url.to_string(),
        title: extracted.title,
        text: Some(extracted.text),
        status: Some(status),
        byte_count: Some(byte_count),
        truncated,
        captured_at: Timestamp::now(),
    })
}

fn validate_http_url(url: &str) -> Result<(), WebError> {
    if url == "about:blank" {
        return Ok(());
    }

    if url.starts_with("http://") || url.starts_with("https://") {
        return Ok(());
    }

    Err(WebError::InvalidUrl(
        "only http://, https://, and about:blank are supported by the text backend".to_string(),
    ))
}

#[derive(Debug, Default)]
pub struct NullBrowserBackend {
    current_url: Option<String>,
}

impl BrowserBackend for NullBrowserBackend {
    fn engine(&self) -> BrowserEngine {
        BrowserEngine::TextPreview
    }

    fn open(&mut self, url: &str) -> Result<(), WebError> {
        self.current_url = Some(url.to_string());
        Ok(())
    }

    fn navigate(&mut self, url: &str) -> Result<(), WebError> {
        self.current_url = Some(url.to_string());
        Ok(())
    }

    fn reload(&mut self) -> Result<(), WebError> {
        Ok(())
    }

    fn go_back(&mut self) -> Result<(), WebError> {
        Ok(())
    }

    fn go_forward(&mut self) -> Result<(), WebError> {
        Ok(())
    }

    fn resize(&mut self, _size: ViewportSize) -> Result<(), WebError> {
        Ok(())
    }

    fn input(&mut self, _input: BrowserInput) -> Result<(), WebError> {
        Ok(())
    }

    fn snapshot(&mut self) -> Result<BrowserBackendSnapshot, WebError> {
        Ok(BrowserBackendSnapshot::from_page(PageSnapshot {
            url: self
                .current_url
                .clone()
                .unwrap_or_else(|| "about:blank".to_string()),
            title: Some("Null Browser".to_string()),
            text: None,
            status: None,
            byte_count: None,
            truncated: false,
            captured_at: Timestamp::now(),
        }))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ExtractedText {
    title: Option<String>,
    text: String,
}

fn extract_html_text(input: &str) -> ExtractedText {
    let title = extract_title(input);
    let without_non_content = remove_tagged_sections(input, &["script", "style", "noscript"]);
    let mut text = String::with_capacity(without_non_content.len());
    let mut in_tag = false;
    let mut previous_was_space = true;

    for ch in without_non_content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                push_space(&mut text, &mut previous_was_space);
            }
            '>' => {
                in_tag = false;
                push_space(&mut text, &mut previous_was_space);
            }
            _ if in_tag => {}
            _ if ch.is_whitespace() => push_space(&mut text, &mut previous_was_space),
            _ => {
                text.push(ch);
                previous_was_space = false;
            }
        }
    }

    ExtractedText {
        title,
        text: decode_basic_html_entities(text.trim()).to_string(),
    }
}

fn extract_title(input: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let title_start = lower.find("<title")?;
    let tag_end_offset = lower[title_start..].find('>')?;
    let content_start = title_start + tag_end_offset + 1;
    let content_end = lower[content_start..].find("</title>")? + content_start;
    let title = decode_basic_html_entities(input[content_start..content_end].trim());
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn remove_tagged_sections(input: &str, tags: &[&str]) -> String {
    let mut output = input.to_string();
    for tag in tags {
        output = remove_tagged_section(&output, tag);
    }
    output
}

fn remove_tagged_section(input: &str, tag: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_open) = lower[cursor..].find(&open_prefix) {
        let open = cursor + relative_open;
        output.push_str(&input[cursor..open]);
        let Some(relative_close) = lower[open..].find(&close_tag) else {
            cursor = input.len();
            break;
        };
        cursor = open + relative_close + close_tag.len();
    }

    output.push_str(&input[cursor..]);
    output
}

fn push_space(text: &mut String, previous_was_space: &mut bool) {
    if !*previous_was_space {
        text.push(' ');
        *previous_was_space = true;
    }
}

fn decode_basic_html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BrowserWorkerCommand {
    Open { url: String },
    Navigate { url: String },
    Resize(ViewportSize),
    Input(BrowserInput),
    Action(BrowserAction),
    Snapshot,
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum BrowserWorkerEvent {
    Snapshot(BrowserBackendSnapshot),
    Frame(BrowserFrameSnapshot),
    Error(String),
    ActionResult(Option<String>),
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::{
        BrowserAction, BrowserEngine, BrowserFrameFormat, BrowserFrameSnapshot, BrowserLoadStatus,
        BrowserSessionState, PageSnapshot, Timestamp, extract_html_text, validate_http_url,
    };

    #[test]
    fn extracts_title_and_visible_text_from_html() {
        let html = r#"
            <!doctype html>
            <html>
              <head>
                <title>Example &amp; AgentHouse</title>
                <style>.hidden { display: none; }</style>
                <script>window.secret = "ignore";</script>
              </head>
              <body>
                <h1>Example Domain</h1>
                <p>Visible&nbsp;text for agents.</p>
              </body>
            </html>
        "#;

        let extracted = extract_html_text(html);

        assert_eq!(extracted.title.as_deref(), Some("Example & AgentHouse"));
        assert!(extracted.text.contains("Example & AgentHouse"));
        assert!(extracted.text.contains("Example Domain"));
        assert!(extracted.text.contains("Visible text for agents."));
        assert!(!extracted.text.contains("window.secret"));
        assert!(!extracted.text.contains("display: none"));
    }

    #[test]
    fn accepts_only_http_https_and_blank_urls() {
        assert!(validate_http_url("https://example.com").is_ok());
        assert!(validate_http_url("http://example.com").is_ok());
        assert!(validate_http_url("about:blank").is_ok());
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn browser_session_state_tracks_snapshot_lifecycle() {
        let mut session =
            BrowserSessionState::new("Browser", "about:blank", BrowserEngine::TextPreview);

        session.mark_loading();
        assert_eq!(session.status, BrowserLoadStatus::Loading);

        session.apply_snapshot(PageSnapshot {
            url: "https://example.com/".to_string(),
            title: Some("Example Domain".to_string()),
            text: Some("Example Domain".to_string()),
            status: Some(200),
            byte_count: Some(42),
            truncated: false,
            captured_at: Timestamp::now(),
        });

        assert_eq!(session.status, BrowserLoadStatus::Loaded);
        assert_eq!(session.current_url, "https://example.com/");
        assert_eq!(session.title, "Example Domain");
        assert!(session.last_error.is_none());
    }

    #[test]
    fn browser_action_marks_selector_actions_as_control_backend_work() {
        assert!(!BrowserAction::navigate("https://example.com").requires_control_backend());
        assert!(
            BrowserAction::Click {
                selector: "button".to_string()
            }
            .requires_control_backend()
        );
        assert!(
            BrowserAction::Evaluate {
                expression: "document.title".to_string()
            }
            .requires_control_backend()
        );
    }

    #[test]
    fn frame_snapshot_carries_renderable_bytes() {
        let frame = BrowserFrameSnapshot::png(2, 1, vec![137, 80, 78, 71]);

        assert_eq!(frame.format, BrowserFrameFormat::Png);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.byte_count, 4);
        assert_eq!(frame.bytes, vec![137, 80, 78, 71]);

        let value = serde_json::to_value(&frame).expect("frame should serialize");
        assert_eq!(value["format"], "png");
        assert_eq!(value["byte_count"], 4);
        assert_eq!(value["bytes"], serde_json::json!([137, 80, 78, 71]));
    }
}
