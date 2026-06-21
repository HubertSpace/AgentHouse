use std::path::PathBuf;

use ah_core::{ProjectId, SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionKind {
    Shell,
    ExternalAgent { command: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionStatus {
    Starting,
    Running,
    Blocked { reason: String },
    Exited { code: Option<i32> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Session {
    pub id: SessionId,
    pub project_id: Option<ProjectId>,
    pub name: String,
    pub cwd: PathBuf,
    pub kind: SessionKind,
    pub status: SessionStatus,
    pub created_at: Timestamp,
}

impl Session {
    #[must_use]
    pub fn shell(name: impl Into<String>, cwd: PathBuf) -> Self {
        Self {
            id: SessionId::new(),
            project_id: None,
            name: name.into(),
            cwd,
            kind: SessionKind::Shell,
            status: SessionStatus::Starting,
            created_at: Timestamp::now(),
        }
    }

    #[must_use]
    pub fn external_agent(
        name: impl Into<String>,
        cwd: PathBuf,
        command: impl Into<String>,
    ) -> Self {
        Self {
            id: SessionId::new(),
            project_id: None,
            name: name.into(),
            cwd,
            kind: SessionKind::ExternalAgent {
                command: command.into(),
            },
            status: SessionStatus::Starting,
            created_at: Timestamp::now(),
        }
    }

    pub fn mark_running(&mut self) {
        self.status = SessionStatus::Running;
    }

    pub fn mark_exited(&mut self, code: Option<i32>) {
        self.status = SessionStatus::Exited { code };
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SessionEvent {
    Created(Session),
    Activated(SessionId),
    StatusChanged {
        session_id: SessionId,
        status: SessionStatus,
    },
    Closed(SessionId),
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session not found")]
    NotFound,
}
