use std::path::PathBuf;

use ah_core::{Actor, BlockId, SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlockKind {
    Command,
    AgentInput,
    AgentOutput,
    FileRef,
    WebRef,
    System,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlockState {
    Streaming,
    Complete,
    Collapsed,
    Pinned,
    Forwarded { to: SessionId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlockAttachment {
    File { path: PathBuf },
    Url { url: String },
    Image { media_type: String, bytes: Vec<u8> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Block {
    pub id: BlockId,
    pub session_id: SessionId,
    pub actor: Actor,
    pub kind: BlockKind,
    pub text: String,
    pub attachments: Vec<BlockAttachment>,
    pub state: BlockState,
    pub created_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

impl Block {
    #[must_use]
    pub fn new(
        session_id: SessionId,
        actor: Actor,
        kind: BlockKind,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: BlockId::new(),
            session_id,
            actor,
            kind,
            text: text.into(),
            attachments: Vec::new(),
            state: BlockState::Streaming,
            created_at: Timestamp::now(),
            finished_at: None,
        }
    }

    pub fn complete(&mut self) {
        self.state = BlockState::Complete;
        self.finished_at = Some(Timestamp::now());
    }

    pub fn attach(&mut self, attachment: BlockAttachment) {
        self.attachments.push(attachment);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForwardedBlock {
    pub source_session_id: SessionId,
    pub target_session_id: SessionId,
    pub block_id: BlockId,
}

#[derive(Debug, Error)]
pub enum BlockError {
    #[error("block is not editable in current state")]
    NotEditable,
}
