// SQLite database integration for session tracking

use crate::error::{GeminiAudioError, Result};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session status
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum SessionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl SessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Pending => "pending",
            SessionStatus::Processing => "processing",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
        }
    }
}

/// Session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub prompt_id: String,
    pub input_file: String,
    pub input_format: String,
    pub output_file: Option<String>,
    pub output_format: Option<String>,
    pub status: SessionStatus,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub last_retry_at: Option<DateTime<Utc>>,
    pub audio_device: Option<String>,
    pub play_audio: bool,
    pub chunk_size_ms: Option<i32>,
    pub buffer_size_ms: Option<i32>,
    pub log_id: Option<String>,
}

/// Database manager
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(database_path: &PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = database_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| GeminiAudioError::Database(format!("Failed to create database directory: {}", e)))?;
        }

        use sqlx::sqlite::SqliteConnectOptions;
        let connect_options = SqliteConnectOptions::new()
            .filename(&database_path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .map_err(|e| GeminiAudioError::Database(format!("Failed to connect to database: {}", e)))?;

        let db = Self { pool };
        db.create_tables().await?;
        
        Ok(db)
    }

    /// Create database tables
    async fn create_tables(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                prompt_id TEXT NOT NULL,
                input_file TEXT NOT NULL,
                input_format TEXT NOT NULL,
                output_file TEXT,
                output_format TEXT,
                status TEXT NOT NULL,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0,
                last_retry_at TIMESTAMP,
                audio_device TEXT,
                play_audio BOOLEAN DEFAULT true,
                chunk_size_ms INTEGER,
                buffer_size_ms INTEGER,
                log_id TEXT
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to create sessions table: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS prompts (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                last_used_at TIMESTAMP
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to create prompts table: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS audio_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                audio_data BLOB NOT NULL,
                mime_type TEXT NOT NULL,
                chunk_index INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to create audio_results table: {}", e)))?;

        Ok(())
    }

    /// Create a new session
    pub async fn create_session(&self, session: &Session) -> Result<i64> {
        let result = sqlx::query(r#"
            INSERT INTO sessions (
                prompt_id, input_file, input_format, output_file, output_format,
                status, error_message, retry_count, last_retry_at, audio_device,
                play_audio, chunk_size_ms, buffer_size_ms, log_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#)
        .bind(&session.prompt_id)
        .bind(&session.input_file)
        .bind(&session.input_format)
        .bind(&session.output_file)
        .bind(&session.output_format)
        .bind(session.status.as_str())
        .bind(&session.error_message)
        .bind(session.retry_count)
        .bind(&session.last_retry_at)
        .bind(&session.audio_device)
        .bind(session.play_audio)
        .bind(session.chunk_size_ms)
        .bind(session.buffer_size_ms)
        .bind(&session.log_id)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to create session: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    /// Update session status
    pub async fn update_session_status(&self, session_id: i64, status: SessionStatus, error_message: Option<String>) -> Result<()> {
        sqlx::query(r#"
            UPDATE sessions 
            SET status = ?, error_message = ?, last_retry_at = CURRENT_TIMESTAMP
            WHERE id = ?
        "#)
        .bind(status.as_str())
        .bind(&error_message)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to update session status: {}", e)))?;

        Ok(())
    }

    /// Increment retry count
    pub async fn increment_retry_count(&self, session_id: i64) -> Result<i32> {
        let row = sqlx::query(r#"
            UPDATE sessions 
            SET retry_count = retry_count + 1
            WHERE id = ?
            RETURNING retry_count
        "#)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to increment retry count: {}", e)))?;

        let retry_count: i32 = row.get("retry_count");
        Ok(retry_count)
    }

    /// Get session by ID
    pub async fn get_session(&self, session_id: i64) -> Result<Option<Session>> {
        let row = sqlx::query(r#"
            SELECT * FROM sessions WHERE id = ?
        "#)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to get session: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(Session {
                id: Some(row.get("id")),
                created_at: row.get("created_at"),
                prompt_id: row.get("prompt_id"),
                input_file: row.get("input_file"),
                input_format: row.get("input_format"),
                output_file: row.get("output_file"),
                output_format: row.get("output_format"),
                status: SessionStatus::Processing, // TODO: Parse from string
                error_message: row.get("error_message"),
                retry_count: row.get("retry_count"),
                last_retry_at: row.get("last_retry_at"),
                audio_device: row.get("audio_device"),
                play_audio: row.get("play_audio"),
                chunk_size_ms: row.get("chunk_size_ms"),
                buffer_size_ms: row.get("buffer_size_ms"),
                log_id: row.get("log_id"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Save prompt
    pub async fn save_prompt(&self, prompt_id: &str, content: &str) -> Result<()> {
        sqlx::query(r#"
            INSERT INTO prompts (id, content) 
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET content = ?, last_used_at = CURRENT_TIMESTAMP
        "#)
        .bind(prompt_id)
        .bind(content)
        .bind(content)
        .execute(&self.pool)
        .await
        .map_err(|e| GeminiAudioError::Database(format!("Failed to save prompt: {}", e)))?;

        Ok(())
    }

    /// Close database connection
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_database_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(temp_file.path());
        
        let db = Database::new(&path).await.unwrap();
        db.close().await;
    }

    #[tokio::test]
    async fn test_session_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = PathBuf::from(temp_file.path());
        
        let db = Database::new(&path).await.unwrap();
        
        let session = Session {
            id: None,
            created_at: Utc::now(),
            prompt_id: "test".to_string(),
            input_file: "test.ogg".to_string(),
            input_format: "ogg".to_string(),
            output_file: None,
            output_format: None,
            status: SessionStatus::Pending,
            error_message: None,
            retry_count: 0,
            last_retry_at: None,
            audio_device: None,
            play_audio: true,
            chunk_size_ms: Some(3000),
            buffer_size_ms: Some(500),
            log_id: None,
        };
        
        let session_id = db.create_session(&session).await.unwrap();
        assert!(session_id > 0);
        
        db.close().await;
    }
}
