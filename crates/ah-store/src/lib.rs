use std::path::Path;

use ah_block::Block;
use ah_control::PersistedControlState;
use ah_core::{SessionId, WorkspaceId};
use ah_session::Session;
use ah_workspace::Workspace;
use rusqlite::{Connection, params};
use thiserror::Error;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn save_session(&self, session: &Session) -> Result<(), StoreError> {
        let json = serde_json::to_string(session)?;
        self.connection.execute(
            "insert or replace into sessions (id, json) values (?1, ?2)",
            params![format!("{:?}", session.id), json],
        )?;
        Ok(())
    }

    pub fn save_block(&self, block: &Block) -> Result<(), StoreError> {
        let json = serde_json::to_string(block)?;
        self.connection.execute(
            "insert or replace into blocks (id, session_id, json) values (?1, ?2, ?3)",
            params![
                format!("{:?}", block.id),
                format!("{:?}", block.session_id),
                json
            ],
        )?;
        Ok(())
    }

    pub fn save_workspace(&self, workspace: &Workspace) -> Result<(), StoreError> {
        let json = serde_json::to_string(workspace)?;
        self.connection.execute(
            "insert or replace into workspaces (id, json) values (?1, ?2)",
            params![format!("{:?}", workspace.id), json],
        )?;
        Ok(())
    }

    pub fn delete_workspace(&self, workspace_id: WorkspaceId) -> Result<(), StoreError> {
        self.connection.execute(
            "delete from workspaces where id = ?1",
            params![format!("{workspace_id:?}")],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, session_id: SessionId) -> Result<(), StoreError> {
        let session_id = format!("{session_id:?}");
        self.connection.execute(
            "delete from blocks where session_id = ?1",
            params![session_id.as_str()],
        )?;
        self.connection
            .execute("delete from sessions where id = ?1", params![session_id])?;
        Ok(())
    }

    pub fn load_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let mut statement = self
            .connection
            .prepare("select json from workspaces order by rowid asc")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut workspaces = Vec::new();
        for row in rows {
            workspaces.push(serde_json::from_str(&row?)?);
        }
        Ok(workspaces)
    }

    pub fn load_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let mut statement = self
            .connection
            .prepare("select json from sessions order by rowid asc")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(serde_json::from_str(&row?)?);
        }
        Ok(sessions)
    }

    pub fn load_session(&self, session_id: SessionId) -> Result<Option<Session>, StoreError> {
        let mut statement = self
            .connection
            .prepare("select json from sessions where id = ?1")?;
        let mut rows = statement.query(params![format!("{session_id:?}")])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let json: String = row.get(0)?;
        Ok(Some(serde_json::from_str(&json)?))
    }

    pub fn load_blocks_for_session(&self, session: &Session) -> Result<Vec<Block>, StoreError> {
        let mut statement = self
            .connection
            .prepare("select json from blocks where session_id = ?1 order by rowid desc")?;
        let rows =
            statement.query_map([format!("{:?}", session.id)], |row| row.get::<_, String>(0))?;
        let mut blocks = Vec::new();
        for row in rows {
            blocks.push(serde_json::from_str(&row?)?);
        }
        Ok(blocks)
    }

    pub fn save_control_state(&self, state: &PersistedControlState) -> Result<(), StoreError> {
        let json = serde_json::to_string(state)?;
        self.connection.execute(
            "insert or replace into control_state (id, json) values ('main', ?1)",
            params![json],
        )?;
        Ok(())
    }

    pub fn load_control_state(&self) -> Result<Option<PersistedControlState>, StoreError> {
        let mut statement = self
            .connection
            .prepare("select json from control_state where id = 'main'")?;
        let mut rows = statement.query([])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let json: String = row.get(0)?;
        Ok(Some(serde_json::from_str(&json)?))
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "
            pragma journal_mode = wal;
            create table if not exists schema_meta (
                key text primary key not null,
                value text not null
            );
            insert or replace into schema_meta (key, value) values ('schema_version', '2');
            create table if not exists workspaces (
                id text primary key not null,
                json text not null
            );
            create table if not exists sessions (
                id text primary key not null,
                json text not null
            );
            create table if not exists blocks (
                id text primary key not null,
                session_id text not null,
                json text not null
            );
            create index if not exists idx_blocks_session_id on blocks(session_id);
            create table if not exists control_state (
                id text primary key not null,
                json text not null
            );
            ",
        )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ah_control::{ControlEvent, PersistedControlState, PersistedSessionRing};

    #[test]
    fn persists_control_state() {
        let store = Store::open_memory().expect("memory store should open");
        let session_id = ah_core::SessionId::new();
        let workspace_id = ah_core::WorkspaceId::new();
        let state = PersistedControlState {
            active_workspace_id: Some(workspace_id),
            closed_workspace_ids: Vec::new(),
            pane_layouts: Vec::new(),
            ui_language: Some(ah_control::UiLanguagePreference::En),
            ui_theme_scheme: Some("glass".to_string()),
            ui_theme_mode: Some("light".to_string()),
            next_event_sequence: 12,
            events: vec![ControlEvent {
                sequence: 11,
                level: "info".to_string(),
                topic: "session".to_string(),
                message: "restored".to_string(),
            }],
            session_rings: vec![PersistedSessionRing {
                session_id,
                state: "complete".to_string(),
                summary: "done".to_string(),
                unread_count: 2,
            }],
        };

        store
            .save_control_state(&state)
            .expect("control state should save");

        let restored = store
            .load_control_state()
            .expect("control state should load")
            .expect("control state should exist");

        assert_eq!(restored, state);
    }
}
