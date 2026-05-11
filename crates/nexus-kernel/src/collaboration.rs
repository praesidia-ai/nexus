//! Collaboration Manager — Layer 8 of the Nexus Agent OS.
//!
//! Provides real-time collaboration sessions where multiple users and agents
//! can work together on a project with shared chat, presence tracking, and
//! role-based participation.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Errors from collaboration operations.
#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    #[error("session {session_id} not found")]
    SessionNotFound { session_id: String },

    #[error("user {user_id} is already in session {session_id}")]
    AlreadyJoined {
        session_id: String,
        user_id: String,
    },

    #[error("user {user_id} is not in session {session_id}")]
    NotInSession {
        session_id: String,
        user_id: String,
    },

    #[error("broadcast failed: {0}")]
    BroadcastFailed(String),
}

/// The role a participant holds within a collaboration session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// Session creator with full control.
    Owner,
    /// Can edit and contribute content.
    Editor,
    /// Read-only observer.
    Viewer,
    /// An AI agent participating in the session.
    Agent,
}

/// A participant in a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// The user or agent identifier.
    pub user_id: String,
    /// Display name.
    pub name: String,
    /// Role within the session.
    pub role: ParticipantRole,
    /// When the participant connected.
    pub connected_at: DateTime<Utc>,
    /// When the participant was last active.
    pub last_active: DateTime<Utc>,
}

/// A message within a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollaborationMessage {
    /// A message from a human user.
    UserMessage {
        user_id: String,
        content: String,
        timestamp: DateTime<Utc>,
    },
    /// A message from an AI agent process.
    AgentMessage {
        pid: String,
        content: String,
        timestamp: DateTime<Utc>,
    },
    /// A system-generated message (joins, leaves, status changes).
    SystemMessage {
        content: String,
        timestamp: DateTime<Utc>,
    },
}

/// A collaboration session where users and agents work together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSession {
    /// Unique session identifier.
    pub id: String,
    /// The project this session is associated with.
    pub project_id: String,
    /// Current participants.
    pub participants: Vec<Participant>,
    /// Chat history.
    pub chat: Vec<CollaborationMessage>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
}

/// Manages collaboration sessions with in-memory storage and broadcast notifications.
pub struct CollaborationManager {
    /// Active sessions keyed by session ID.
    sessions: RwLock<HashMap<String, CollaborationSession>>,
    /// Broadcast sender for session events (serialized as JSON strings).
    event_tx: broadcast::Sender<String>,
}

impl std::fmt::Debug for CollaborationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollaborationManager")
            .field("sessions", &"<RwLock>")
            .field("event_tx", &"<broadcast::Sender>")
            .finish()
    }
}

impl CollaborationManager {
    /// Create a new collaboration manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            sessions: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Create a new collaboration session and return its ID.
    pub async fn create_session(&self, project_id: &str, creator: Participant) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let session = CollaborationSession {
            id: id.clone(),
            project_id: project_id.to_string(),
            participants: vec![creator],
            chat: vec![CollaborationMessage::SystemMessage {
                content: "Session created.".to_string(),
                timestamp: now,
            }],
            created_at: now,
        };
        self.sessions.write().await.insert(id.clone(), session);
        let _ = self.event_tx.send(
            serde_json::json!({
                "event": "session_created",
                "session_id": &id,
                "project_id": project_id,
            })
            .to_string(),
        );
        id
    }

    /// Add a participant to an existing session.
    pub async fn join_session(
        &self,
        session_id: &str,
        participant: Participant,
    ) -> Result<(), CollabError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| CollabError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;

        if session
            .participants
            .iter()
            .any(|p| p.user_id == participant.user_id)
        {
            return Err(CollabError::AlreadyJoined {
                session_id: session_id.to_string(),
                user_id: participant.user_id,
            });
        }

        let name = participant.name.clone();
        session.participants.push(participant);
        session.chat.push(CollaborationMessage::SystemMessage {
            content: format!("{name} joined the session."),
            timestamp: Utc::now(),
        });

        let _ = self.event_tx.send(
            serde_json::json!({
                "event": "participant_joined",
                "session_id": session_id,
                "name": name,
            })
            .to_string(),
        );
        Ok(())
    }

    /// Remove a participant from a session.
    pub async fn leave_session(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<(), CollabError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| CollabError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;

        let idx = session
            .participants
            .iter()
            .position(|p| p.user_id == user_id)
            .ok_or_else(|| CollabError::NotInSession {
                session_id: session_id.to_string(),
                user_id: user_id.to_string(),
            })?;

        let removed = session.participants.remove(idx);
        session.chat.push(CollaborationMessage::SystemMessage {
            content: format!("{} left the session.", removed.name),
            timestamp: Utc::now(),
        });

        let _ = self.event_tx.send(
            serde_json::json!({
                "event": "participant_left",
                "session_id": session_id,
                "user_id": user_id,
            })
            .to_string(),
        );
        Ok(())
    }

    /// Send a message to a session's chat.
    pub async fn send_message(
        &self,
        session_id: &str,
        message: CollaborationMessage,
    ) -> Result<(), CollabError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| CollabError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;

        session.chat.push(message.clone());

        let _ = self.event_tx.send(
            serde_json::json!({
                "event": "new_message",
                "session_id": session_id,
                "message": message,
            })
            .to_string(),
        );
        Ok(())
    }

    /// Get a snapshot of a session.
    pub async fn get_session(&self, session_id: &str) -> Option<CollaborationSession> {
        self.sessions.read().await.get(session_id).cloned()
    }

    /// List all active sessions.
    pub async fn list_sessions(&self) -> Vec<CollaborationSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// Subscribe to collaboration events. Returns a broadcast receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.event_tx.subscribe()
    }
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_participant(id: &str, name: &str, role: ParticipantRole) -> Participant {
        Participant {
            user_id: id.to_string(),
            name: name.to_string(),
            role,
            connected_at: Utc::now(),
            last_active: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let mgr = CollaborationManager::new();
        let creator = make_participant("u1", "Alice", ParticipantRole::Owner);
        let sid = mgr.create_session("proj-1", creator).await;

        let session = mgr.get_session(&sid).await.unwrap();
        assert_eq!(session.project_id, "proj-1");
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].name, "Alice");
    }

    #[tokio::test]
    async fn test_join_and_leave() {
        let mgr = CollaborationManager::new();
        let creator = make_participant("u1", "Alice", ParticipantRole::Owner);
        let sid = mgr.create_session("proj-1", creator).await;

        let bob = make_participant("u2", "Bob", ParticipantRole::Editor);
        mgr.join_session(&sid, bob).await.unwrap();

        let session = mgr.get_session(&sid).await.unwrap();
        assert_eq!(session.participants.len(), 2);

        mgr.leave_session(&sid, "u2").await.unwrap();
        let session = mgr.get_session(&sid).await.unwrap();
        assert_eq!(session.participants.len(), 1);
    }

    #[tokio::test]
    async fn test_duplicate_join_fails() {
        let mgr = CollaborationManager::new();
        let creator = make_participant("u1", "Alice", ParticipantRole::Owner);
        let sid = mgr.create_session("proj-1", creator).await;

        let dup = make_participant("u1", "Alice", ParticipantRole::Editor);
        let result = mgr.join_session(&sid, dup).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_message() {
        let mgr = CollaborationManager::new();
        let creator = make_participant("u1", "Alice", ParticipantRole::Owner);
        let sid = mgr.create_session("proj-1", creator).await;

        let msg = CollaborationMessage::UserMessage {
            user_id: "u1".to_string(),
            content: "Hello!".to_string(),
            timestamp: Utc::now(),
        };
        mgr.send_message(&sid, msg).await.unwrap();

        let session = mgr.get_session(&sid).await.unwrap();
        // 1 system message (creation) + 1 user message
        assert_eq!(session.chat.len(), 2);
    }

    #[tokio::test]
    async fn test_session_not_found() {
        let mgr = CollaborationManager::new();
        assert!(mgr.get_session("nonexistent").await.is_none());

        let participant = make_participant("u1", "Alice", ParticipantRole::Viewer);
        let result = mgr.join_session("nonexistent", participant).await;
        assert!(result.is_err());
    }
}
