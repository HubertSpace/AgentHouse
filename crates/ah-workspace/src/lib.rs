use std::path::PathBuf;

use ah_core::{SessionId, TabId, Timestamp, WindowId, WorkspaceId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root: Option<PathBuf>,
    pub windows: Vec<WorkspaceWindow>,
    pub active_window_id: Option<WindowId>,
    pub layout: WorkspaceLayout,
    pub created_at: Timestamp,
}

impl Workspace {
    #[must_use]
    pub fn new(name: impl Into<String>, root: Option<PathBuf>) -> Self {
        Self {
            id: WorkspaceId::new(),
            name: name.into(),
            root,
            windows: Vec::new(),
            active_window_id: None,
            layout: WorkspaceLayout::single(),
            created_at: Timestamp::now(),
        }
    }

    pub fn push_window(&mut self, window: WorkspaceWindow) {
        self.active_window_id = Some(window.id);
        self.windows.push(window);
    }

    pub fn active_window_mut(&mut self) -> Option<&mut WorkspaceWindow> {
        let active_window_id = self.active_window_id?;
        self.windows
            .iter_mut()
            .find(|window| window.id == active_window_id)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceLayout {
    pub mode: LayoutMode,
}

impl WorkspaceLayout {
    #[must_use]
    pub fn single() -> Self {
        Self {
            mode: LayoutMode::Single,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    Single,
    Columns,
    Grid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceWindow {
    pub id: WindowId,
    pub title: String,
    pub tabs: Vec<WindowTab>,
    pub active_tab_id: Option<TabId>,
    pub created_at: Timestamp,
}

impl WorkspaceWindow {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: WindowId::new(),
            title: title.into(),
            tabs: Vec::new(),
            active_tab_id: None,
            created_at: Timestamp::now(),
        }
    }

    #[must_use]
    pub fn terminal(title: impl Into<String>, session_id: SessionId) -> Self {
        let mut window = Self::new("Main");
        window.push_tab(WindowTab::terminal(title, session_id));
        window
    }

    #[must_use]
    pub fn web(title: impl Into<String>, url: impl Into<String>) -> Self {
        let mut window = Self::new("Main");
        window.push_tab(WindowTab::web(title, url));
        window
    }

    #[must_use]
    pub fn file_preview(title: impl Into<String>, path: PathBuf) -> Self {
        let mut window = Self::new("Main");
        window.push_tab(WindowTab::file_preview(title, path));
        window
    }

    pub fn push_tab(&mut self, tab: WindowTab) {
        self.active_tab_id = Some(tab.id);
        self.tabs.push(tab);
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<&WindowTab> {
        self.active_tab_id
            .and_then(|tab_id| self.tabs.iter().find(|tab| tab.id == tab_id))
            .or_else(|| self.tabs.first())
    }

    pub fn activate_tab(&mut self, tab_id: TabId) -> bool {
        if self.tabs.iter().any(|tab| tab.id == tab_id) {
            self.active_tab_id = Some(tab_id);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WindowTab {
    pub id: TabId,
    pub title: String,
    pub content: WindowContent,
    pub created_at: Timestamp,
}

impl WindowTab {
    #[must_use]
    pub fn terminal(title: impl Into<String>, session_id: SessionId) -> Self {
        Self {
            id: TabId::new(),
            title: title.into(),
            content: WindowContent::Terminal { session_id },
            created_at: Timestamp::now(),
        }
    }

    #[must_use]
    pub fn web(title: impl Into<String>, url: impl Into<String>) -> Self {
        let session_id = SessionId::new();
        Self::web_with_session(title, session_id, url)
    }

    #[must_use]
    pub fn web_with_session(
        title: impl Into<String>,
        session_id: SessionId,
        url: impl Into<String>,
    ) -> Self {
        Self {
            id: TabId::new(),
            title: title.into(),
            content: WindowContent::Web {
                session_id,
                url: url.into(),
            },
            created_at: Timestamp::now(),
        }
    }

    #[must_use]
    pub fn file_preview(title: impl Into<String>, path: PathBuf) -> Self {
        Self {
            id: TabId::new(),
            title: title.into(),
            content: WindowContent::FilePreview { path },
            created_at: Timestamp::now(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WindowContent {
    Terminal { session_id: SessionId },
    Web { session_id: SessionId, url: String },
    FilePreview { path: PathBuf },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkspaceEvent {
    Created(Workspace),
    WindowOpened {
        workspace_id: WorkspaceId,
        window: WorkspaceWindow,
    },
    WindowActivated {
        workspace_id: WorkspaceId,
        window_id: WindowId,
    },
    WindowClosed {
        workspace_id: WorkspaceId,
        window_id: WindowId,
    },
}

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("workspace not found")]
    WorkspaceNotFound,
    #[error("window not found")]
    WindowNotFound,
    #[error("tab not found")]
    TabNotFound,
}

#[cfg(test)]
mod tests {
    use super::{LayoutMode, WindowContent, WindowTab, Workspace, WorkspaceWindow};
    use ah_core::SessionId;

    #[test]
    fn workspace_window_tracks_active_tab() {
        let first_session = SessionId::new();
        let second_session = SessionId::new();
        let mut window = WorkspaceWindow::new("Main Window");
        let first = WindowTab::terminal("first", first_session);
        let first_tab_id = first.id;
        let second = WindowTab::terminal("second", second_session);
        let second_tab_id = second.id;

        window.push_tab(first);
        window.push_tab(second);

        assert_eq!(window.active_tab().map(|tab| tab.id), Some(second_tab_id));
        assert!(window.activate_tab(first_tab_id));
        assert_eq!(window.active_tab().map(|tab| tab.id), Some(first_tab_id));

        let active_tab = window.active_tab().expect("active tab should exist");
        assert_eq!(
            active_tab.content,
            WindowContent::Terminal {
                session_id: first_session
            }
        );
    }

    #[test]
    fn compatibility_constructors_create_single_tab_windows() {
        let session_id = SessionId::new();
        let window = WorkspaceWindow::terminal("Shell", session_id);

        assert_eq!(window.tabs.len(), 1);
        assert_eq!(
            window.active_tab().map(|tab| tab.title.as_str()),
            Some("Shell")
        );
        assert_eq!(
            window.active_tab().map(|tab| &tab.content),
            Some(&WindowContent::Terminal { session_id })
        );
    }

    #[test]
    fn web_tabs_bind_to_browser_session_ids() {
        let browser_session_id = SessionId::new();
        let tab = WindowTab::web_with_session("Browser", browser_session_id, "https://example.com");

        assert_eq!(
            tab.content,
            WindowContent::Web {
                session_id: browser_session_id,
                url: "https://example.com".to_string()
            }
        );

        let generated = WindowTab::web("Browser", "about:blank");
        assert!(matches!(
            generated.content,
            WindowContent::Web {
                url,
                session_id: _
            } if url == "about:blank"
        ));
    }

    #[test]
    fn workspace_push_window_makes_it_active() {
        let mut workspace = Workspace::new("Workspace", None);
        let window = WorkspaceWindow::new("Main Window");
        let window_id = window.id;

        workspace.push_window(window);

        assert_eq!(workspace.active_window_id, Some(window_id));
        assert_eq!(
            workspace.active_window_mut().map(|window| window.id),
            Some(window_id)
        );
    }

    #[test]
    fn workspace_defaults_to_single_layout() {
        let workspace = Workspace::new("Workspace", None);

        assert_eq!(workspace.layout.mode, LayoutMode::Single);
    }
}
