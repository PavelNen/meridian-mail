use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::models::*;

// ─── Schema ───────────────────────────────────────────────────────────────────

const SCHEMA_V1: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    email        TEXT NOT NULL UNIQUE,
    display_name TEXT,
    imap_host    TEXT NOT NULL,
    imap_port    INTEGER NOT NULL DEFAULT 993,
    smtp_host    TEXT NOT NULL,
    smtp_port    INTEGER NOT NULL DEFAULT 587,
    keychain_ref TEXT NOT NULL,
    created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS contacts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    email        TEXT NOT NULL UNIQUE,
    display_name TEXT,
    avatar_url   TEXT,
    last_seen    DATETIME,
    created_at   DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS conversations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    kind            TEXT NOT NULL CHECK (kind IN ('direct', 'group', 'channel')),
    subject_hint    TEXT,
    last_message_at DATETIME,
    unread_count    INTEGER NOT NULL DEFAULT 0,
    created_at      DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS threads (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    subject         TEXT NOT NULL,
    normalized_subject TEXT NOT NULL,
    last_message_at DATETIME,
    unread_count    INTEGER NOT NULL DEFAULT 0,
    created_at      DATETIME NOT NULL DEFAULT (datetime('now')),
    UNIQUE(conversation_id, normalized_subject)
);

CREATE TABLE IF NOT EXISTS conversation_members (
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    contact_id      INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('to', 'cc', 'bcc')),
    PRIMARY KEY (conversation_id, contact_id)
);

CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    thread_id       INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    message_id      TEXT NOT NULL UNIQUE,
    in_reply_to     TEXT,
    from_email      TEXT NOT NULL,
    from_name       TEXT,
    subject         TEXT,
    body_text       TEXT,
    body_html       TEXT,
    sent_at         DATETIME NOT NULL,
    is_outgoing     BOOLEAN NOT NULL DEFAULT 0,
    is_read         BOOLEAN NOT NULL DEFAULT 0,
    raw_headers     TEXT,
    created_at      DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS attachments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id  INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename    TEXT,
    mime_type   TEXT,
    size_bytes  INTEGER,
    local_path  TEXT,
    created_at  DATETIME NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_conv     ON messages(conversation_id, sent_at);
CREATE INDEX IF NOT EXISTS idx_messages_thread   ON messages(thread_id, sent_at);
CREATE INDEX IF NOT EXISTS idx_messages_msgid    ON messages(message_id);
CREATE INDEX IF NOT EXISTS idx_messages_reply    ON messages(in_reply_to);
CREATE INDEX IF NOT EXISTS idx_conv_last         ON conversations(last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_threads_last      ON threads(last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_conv_members_conv ON conversation_members(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conv_members_cont ON conversation_members(contact_id);
"#;

const CURRENT_VERSION: i64 = 2;

// ─── Database handle ──────────────────────────────────────────────────────────

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Открыть (или создать) базу данных по указанному пути.
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let mut db = Self { conn };
        db.setup_connection()?;
        db.migrate()?;
        Ok(db)
    }

    /// In-memory база для тестов.
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Self { conn };
        db.setup_connection()?;
        db.migrate()?;
        Ok(db)
    }

    /// Configure connection-level SQLite settings.
    /// Must be called after every `Connection::open` since PRAGMAs are per-connection.
    fn setup_connection(&mut self) -> AppResult<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }

    fn migrate(&mut self) -> AppResult<()> {
        // Получаем текущую версию схемы
        let version: i64 = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
            // Upsert версии
            self.conn.execute_batch(&format!(
                "DELETE FROM schema_version; INSERT INTO schema_version VALUES ({CURRENT_VERSION});"
            ))?;
        }

        Ok(())
    }
}

// ─── Accounts ─────────────────────────────────────────────────────────────────

impl Database {
    pub fn insert_account(&self, acc: &NewAccount, keychain_ref: &str) -> AppResult<i64> {
        self.conn.execute(
            r#"INSERT INTO accounts
               (email, display_name, imap_host, imap_port, smtp_host, smtp_port, keychain_ref)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            params![
                acc.email,
                acc.display_name,
                acc.imap_host,
                acc.imap_port,
                acc.smtp_host,
                acc.smtp_port,
                keychain_ref,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_account(&self, id: i64) -> AppResult<Account> {
        let acc = self.conn.query_row(
            "SELECT id, email, display_name, imap_host, imap_port, smtp_host, smtp_port, keychain_ref
             FROM accounts WHERE id = ?1",
            params![id],
            row_to_account,
        )?;
        Ok(acc)
    }

    pub fn get_account_by_email(&self, email: &str) -> AppResult<Account> {
        let acc = self.conn.query_row(
            "SELECT id, email, display_name, imap_host, imap_port, smtp_host, smtp_port, keychain_ref
             FROM accounts WHERE email = ?1",
            params![email],
            row_to_account,
        )?;
        Ok(acc)
    }

    pub fn list_accounts(&self) -> AppResult<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, email, display_name, imap_host, imap_port, smtp_host, smtp_port, keychain_ref
             FROM accounts ORDER BY id"
        )?;
        let rows = stmt.query_map([], row_to_account)?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }

    pub fn delete_account(&self, id: i64) -> AppResult<()> {
        // Explicitly cascade-delete child records so deletion works even if
        // FK enforcement is somehow disabled at the connection level.
        self.conn.execute(
            "DELETE FROM messages WHERE account_id = ?1",
            params![id],
        )?;
        // Clean up conversation rows that no longer have any messages
        self.conn.execute(
            "DELETE FROM conversations WHERE id NOT IN (SELECT DISTINCT conversation_id FROM messages)",
            [],
        )?;
        let rows = self.conn.execute(
            "DELETE FROM accounts WHERE id = ?1",
            params![id],
        )?;
        if rows == 0 {
            return Err(AppError::AccountNotFound(id));
        }
        Ok(())
    }

    pub fn delete_message(&self, id: i64) -> AppResult<()> {
        let conv_id: Option<i64> = self.conn.query_row(
            "SELECT conversation_id FROM messages WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).optional()?;

        self.conn.execute(
            "DELETE FROM messages WHERE id = ?1",
            params![id],
        )?;

        if let Some(c_id) = conv_id {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                params![c_id],
                |row| row.get(0),
            )?;
            
            if count == 0 {
                let _ = self.delete_conversation(c_id);
            } else {
                // Find the new latest message to update conversation metadata
                let latest: Option<String> = self.conn.query_row(
                    "SELECT sent_at FROM messages WHERE conversation_id = ?1 ORDER BY sent_at DESC LIMIT 1",
                    params![c_id],
                    |row| row.get(0),
                ).optional()?;

                if let Some(sent_at) = latest {
                    self.conn.execute(
                        "UPDATE conversations SET last_message_at = ?1 WHERE id = ?2",
                        params![sent_at, c_id],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: i64) -> AppResult<()> {
        // Messages and members have ON DELETE CASCADE, so deleting the
        // conversation row removes everything.
        self.conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Return all RFC Message-IDs for a conversation — lightweight, no body fetching.
    pub fn get_message_ids_for_conversation(&self, conversation_id: i64) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT message_id FROM messages WHERE conversation_id = ?1",
        )?;
        let rows = stmt.query_map(params![conversation_id], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }
}

fn row_to_account(row: &rusqlite::Row<'_>) -> SqlResult<Account> {
    Ok(Account {
        id: row.get(0)?,
        email: row.get(1)?,
        display_name: row.get(2)?,
        imap_host: row.get(3)?,
        imap_port: row.get::<_, i64>(4)? as u16,
        smtp_host: row.get(5)?,
        smtp_port: row.get::<_, i64>(6)? as u16,
        keychain_ref: row.get(7)?,
    })
}

// ─── Contacts ─────────────────────────────────────────────────────────────────

impl Database {
    /// Upsert контакта: если email уже есть — обновить display_name и last_seen.
    pub fn upsert_contact(&self, email: &str, display_name: Option<&str>) -> AppResult<i64> {
        self.conn.execute(
            r#"INSERT INTO contacts (email, display_name, last_seen)
               VALUES (?1, ?2, datetime('now'))
               ON CONFLICT(email) DO UPDATE SET
                   display_name = COALESCE(?2, display_name),
                   last_seen    = datetime('now')"#,
            params![email, display_name],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM contacts WHERE email = ?1",
            params![email],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn get_contact_by_email(&self, email: &str) -> AppResult<Option<Contact>> {
        let result = self.conn.query_row(
            "SELECT id, email, display_name, avatar_url, last_seen FROM contacts WHERE email = ?1",
            params![email],
            row_to_contact,
        );
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    pub fn list_contacts(&self) -> AppResult<Vec<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, email, display_name, avatar_url, last_seen FROM contacts ORDER BY display_name, email"
        )?;
        let rows = stmt.query_map([], row_to_contact)?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }

    pub fn search_contacts(&self, query: &str) -> AppResult<Vec<Contact>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            r#"SELECT id, email, display_name, avatar_url, last_seen
               FROM contacts
               WHERE email LIKE ?1 OR display_name LIKE ?1
               ORDER BY display_name, email
               LIMIT 50"#,
        )?;
        let rows = stmt.query_map(params![pattern], row_to_contact)?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }
}

fn row_to_contact(row: &rusqlite::Row<'_>) -> SqlResult<Contact> {
    use chrono::DateTime;
    Ok(Contact {
        id: row.get(0)?,
        email: row.get(1)?,
        display_name: row.get(2)?,
        avatar_url: row.get(3)?,
        last_seen: row
            .get::<_, Option<String>>(4)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
    })
}

// ─── Conversations ────────────────────────────────────────────────────────────

impl Database {
    /// Найти direct-диалог между двумя контактами, или None если не существует.
    pub fn find_direct_conversation(
        &self,
        contact_id_a: i64,
        contact_id_b: i64,
    ) -> AppResult<Option<i64>> {
        let result = self.conn.query_row(
            r#"SELECT c.id
               FROM conversations c
               JOIN conversation_members ma ON ma.conversation_id = c.id AND ma.contact_id = ?1
               JOIN conversation_members mb ON mb.conversation_id = c.id AND mb.contact_id = ?2
               WHERE c.kind = 'direct'
               LIMIT 1"#,
            params![contact_id_a, contact_id_b],
            |r| r.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    /// Найти group-диалог по отсортированному набору contact_id.
    pub fn find_group_conversation(&self, contact_ids: &[i64]) -> AppResult<Option<i64>> {
        if contact_ids.is_empty() {
            return Ok(None);
        }
        let placeholders = contact_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"SELECT conversation_id
               FROM conversation_members
               WHERE contact_id IN ({placeholders})
               GROUP BY conversation_id
               HAVING COUNT(DISTINCT contact_id) = ?
               AND (SELECT COUNT(*) FROM conversation_members m2
                    WHERE m2.conversation_id = conversation_members.conversation_id) = ?
               LIMIT 1"#
        );
        let n = contact_ids.len() as i64;
        let mut stmt = self.conn.prepare(&sql)?;

        let mut bind_params: Vec<Box<dyn rusqlite::ToSql>> = contact_ids
            .iter()
            .map(|id| -> Box<dyn rusqlite::ToSql> { Box::new(*id) })
            .collect();
        bind_params.push(Box::new(n));
        bind_params.push(Box::new(n));

        let refs: Vec<&dyn rusqlite::ToSql> = bind_params.iter().map(|b| b.as_ref()).collect();

        let result = stmt.query_row(refs.as_slice(), |r| r.get(0));
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    pub fn create_conversation(&self, kind: &ConversationKind) -> AppResult<i64> {
        self.conn.execute(
            "INSERT INTO conversations (kind) VALUES (?1)",
            params![kind.as_str()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_conversation_member(
        &self,
        conversation_id: i64,
        contact_id: i64,
        role: &MemberRole,
    ) -> AppResult<()> {
        self.conn.execute(
            r#"INSERT OR IGNORE INTO conversation_members (conversation_id, contact_id, role)
               VALUES (?1, ?2, ?3)"#,
            params![conversation_id, contact_id, role.as_str()],
        )?;
        Ok(())
    }

    pub fn update_conversation_last_message(
        &self,
        conversation_id: i64,
        sent_at: &chrono::DateTime<chrono::Utc>,
        preview: Option<&str>,
    ) -> AppResult<()> {
        let _ = preview; // preview хранится в messages, используется для join
        self.conn.execute(
            "UPDATE conversations SET last_message_at = ?1 WHERE id = ?2",
            params![sent_at.to_rfc3339(), conversation_id],
        )?;
        Ok(())
    }

    pub fn increment_unread(&self, conversation_id: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE conversations SET unread_count = unread_count + 1 WHERE id = ?1",
            params![conversation_id],
        )?;
        Ok(())
    }

    pub fn mark_conversation_read(&self, conversation_id: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE conversations SET unread_count = 0 WHERE id = ?1",
            params![conversation_id],
        )?;
        self.conn.execute(
            "UPDATE messages SET is_read = 1 WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        Ok(())
    }

    /// Список диалогов для главного экрана, отсортированных по дате последнего сообщения.
    pub fn list_conversations(&self, account_id: i64) -> AppResult<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT DISTINCT c.id, c.kind, c.subject_hint, c.last_message_at, c.unread_count
               FROM conversations c
               JOIN messages m ON c.id = m.conversation_id
               WHERE m.account_id = ?1
               ORDER BY c.last_message_at DESC NULLS LAST"#,
        )?;

        let conv_rows: Vec<(i64, String, Option<String>, Option<String>, i64)> = stmt
            .query_map(params![account_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut conversations = Vec::with_capacity(conv_rows.len());
        for (id, kind_str, subject_hint, last_at_str, unread) in conv_rows {
            let kind =
                ConversationKind::try_from(kind_str.as_str()).unwrap_or(ConversationKind::Direct);

            let last_message_at = last_at_str
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            let members = self.get_conversation_members(id)?;

            let last_message_preview = self.get_last_message_preview(id)?;

            conversations.push(Conversation {
                id,
                kind,
                subject_hint,
                last_message_at,
                unread_count: unread,
                members,
                last_message_preview,
            });
        }

        Ok(conversations)
    }

    pub fn get_conversation_members(
        &self,
        conversation_id: i64,
    ) -> AppResult<Vec<ConversationMember>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT c.id, c.email, c.display_name, c.avatar_url, c.last_seen, cm.role
               FROM conversation_members cm
               JOIN contacts c ON c.id = cm.contact_id
               WHERE cm.conversation_id = ?1"#,
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            use chrono::DateTime;
            let contact = Contact {
                id: row.get(0)?,
                email: row.get(1)?,
                display_name: row.get(2)?,
                avatar_url: row.get(3)?,
                last_seen: row
                    .get::<_, Option<String>>(4)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
            };
            let role_str: String = row.get(5)?;
            Ok((contact, role_str))
        })?;

        let mut members = vec![];
        for row in rows {
            let (contact, role_str) = row.map_err(AppError::Database)?;
            let role = MemberRole::try_from(role_str.as_str()).unwrap_or(MemberRole::To);
            members.push(ConversationMember { contact, role });
        }
        Ok(members)
    }

    fn get_last_message_preview(&self, conversation_id: i64) -> AppResult<Option<String>> {
        let result = self.conn.query_row(
            r#"SELECT body_text FROM messages
               WHERE conversation_id = ?1
               ORDER BY sent_at DESC
               LIMIT 1"#,
            params![conversation_id],
            |r| r.get::<_, Option<String>>(0),
        );
        match result {
            Ok(text) => Ok(text.map(|t| {
                let preview: String = t.chars().take(120).collect();
                if t.len() > 120 {
                    format!("{preview}…")
                } else {
                    preview
                }
            })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }
}

// ─── Threads ──────────────────────────────────────────────────────────────────

impl Database {
    pub fn create_thread(
        &self,
        conversation_id: i64,
        subject: &str,
        normalized_subject: &str,
    ) -> AppResult<i64> {
        self.conn.execute(
            r#"INSERT OR IGNORE INTO threads (conversation_id, subject, normalized_subject)
               VALUES (?1, ?2, ?3)"#,
            params![conversation_id, subject, normalized_subject],
        )?;
        
        let id: i64 = self.conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND normalized_subject = ?2",
            params![conversation_id, normalized_subject],
            |r| r.get(0),
        )?;
        
        Ok(id)
    }

    pub fn get_thread(&self, id: i64) -> AppResult<Thread> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, conversation_id, subject, normalized_subject, last_message_at, unread_count
               FROM threads WHERE id = ?1"#,
        )?;
        let t = stmt.query_row(params![id], row_to_thread).map_err(AppError::Database)?;
        Ok(t)
    }

    pub fn get_threads_for_conversation(&self, conversation_id: i64) -> AppResult<Vec<ThreadListItem>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT t.id, t.conversation_id, t.subject, t.last_message_at, t.unread_count,
                      (SELECT body_text FROM messages m WHERE m.thread_id = t.id ORDER BY m.sent_at  DESC LIMIT 1) as last_message_preview
               FROM threads t
               WHERE t.conversation_id = ?1
               ORDER BY t.last_message_at DESC NULLS LAST"#,
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            use chrono::DateTime;
            let last_at_str: Option<String> = row.get(3)?;
            let last_message_at = last_at_str
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            let preview_raw: Option<String> = row.get(5)?;
            let last_message_preview = preview_raw.map(|t| {
                let preview: String = t.chars().take(120).collect();
                if t.len() > 120 { format!("{preview}…") } else { preview }
            });

            Ok(ThreadListItem {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                subject: row.get(2)?,
                last_message_at,
                unread_count: row.get(4)?,
                last_message_preview,
            })
        })?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }

    pub fn update_thread_last_message(
        &self,
        thread_id: i64,
        sent_at: &chrono::DateTime<chrono::Utc>,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE threads SET last_message_at = ?1 WHERE id = ?2",
            params![sent_at.to_rfc3339(), thread_id],
        )?;
        Ok(())
    }

    pub fn increment_thread_unread(&self, thread_id: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE threads SET unread_count = unread_count + 1 WHERE id = ?1",
            params![thread_id],
        )?;
        Ok(())
    }

    pub fn mark_thread_read(&self, thread_id: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE threads SET unread_count = 0 WHERE id = ?1",
            params![thread_id],
        )?;
        self.conn.execute(
            "UPDATE messages SET is_read = 1 WHERE thread_id = ?1",
            params![thread_id],
        )?;
        
        // Recalculate conversation unread count
        let conv_id: Option<i64> = self.conn.query_row(
            "SELECT conversation_id FROM threads WHERE id = ?1 LIMIT 1",
            params![thread_id],
            |r| r.get(0),
        ).optional()?;
        
        if let Some(c_id) = conv_id {
            let total_unread: i64 = self.conn.query_row(
                "SELECT SUM(unread_count) FROM threads WHERE conversation_id = ?1",
                params![c_id],
                |r| r.get(0),
            ).unwrap_or(0);
            
            self.conn.execute(
                "UPDATE conversations SET unread_count = ?1 WHERE id = ?2",
                params![total_unread, c_id],
            )?;
        }

        Ok(())
    }
}

fn row_to_thread(row: &rusqlite::Row<'_>) -> SqlResult<Thread> {
    use chrono::DateTime;
    let last_at_str: Option<String> = row.get(4)?;
    let last_message_at = last_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    Ok(Thread {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        subject: row.get(2)?,
        normalized_subject: row.get(3)?,
        last_message_at,
        unread_count: row.get(5)?,
    })
}

// ─── Messages ─────────────────────────────────────────────────────────────────

impl Database {
    pub fn insert_message(&self, msg: &InsertMessage) -> AppResult<i64> {
        self.conn.execute(
            r#"INSERT OR IGNORE INTO messages
               (conversation_id, thread_id, account_id, message_id, in_reply_to,
                from_email, from_name, subject, body_text, body_html,
                sent_at, is_outgoing, is_read, raw_headers)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"#,
            params![
                msg.conversation_id,
                msg.thread_id,
                msg.account_id,
                msg.message_id,
                msg.in_reply_to,
                msg.from_email,
                msg.from_name,
                msg.subject,
                msg.body_text,
                msg.body_html,
                msg.sent_at.to_rfc3339(),
                msg.is_outgoing,
                msg.is_read,
                msg.raw_headers,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn message_exists(&self, message_id: &str) -> AppResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE message_id = ?1",
            params![message_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }
    /// Fetch a single message by its primary key.
    pub fn get_message_by_id(&self, id: i64) -> AppResult<Message> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                      from_email, from_name, subject, body_text, body_html,
                      sent_at, is_outgoing, is_read, raw_headers, thread_id
               FROM messages WHERE id = ?1"#,
        )?;
        let mut msg = stmt
            .query_row(params![id], row_to_message)
            .map_err(AppError::Database)?;
        msg.attachments = self.get_attachments_for_message(id)?;
        Ok(msg)
    }

    pub fn get_messages_for_conversation(
        &self,
        conversation_id: i64,
        limit: Option<i64>,
        before_id: Option<i64>,
    ) -> AppResult<Vec<Message>> {
        let limit = limit.unwrap_or(50);
        let sql = if let Some(before) = before_id {
            format!(
                r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                          from_email, from_name, subject, body_text, body_html,
                          sent_at, is_outgoing, is_read, raw_headers, thread_id
                   FROM messages
                   WHERE conversation_id = {conversation_id} AND id < {before}
                   ORDER BY sent_at ASC
                   LIMIT {limit}"#
            )
        } else {
            format!(
                r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                          from_email, from_name, subject, body_text, body_html,
                          sent_at, is_outgoing, is_read, raw_headers, thread_id
                   FROM messages
                   WHERE conversation_id = {conversation_id}
                   ORDER BY sent_at ASC
                   LIMIT {limit}"#
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_message)?;
        let mut messages: Vec<Message> = rows
            .map(|r| r.map_err(AppError::Database))
            .collect::<AppResult<Vec<_>>>()?;

        // Подгрузить вложения
        for msg in &mut messages {
            msg.attachments = self.get_attachments_for_message(msg.id)?;
        }

        Ok(messages)
    }

    pub fn get_messages_for_thread(
        &self,
        thread_id: i64,
        limit: Option<i64>,
        before_id: Option<i64>,
    ) -> AppResult<Vec<Message>> {
        let limit = limit.unwrap_or(50);
        let sql = if let Some(before) = before_id {
            format!(
                r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                          from_email, from_name, subject, body_text, body_html,
                          sent_at, is_outgoing, is_read, raw_headers, thread_id
                   FROM messages
                   WHERE thread_id = {thread_id} AND id < {before}
                   ORDER BY sent_at ASC
                   LIMIT {limit}"#
            )
        } else {
            format!(
                r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                          from_email, from_name, subject, body_text, body_html,
                          sent_at, is_outgoing, is_read, raw_headers, thread_id
                   FROM messages
                   WHERE thread_id = {thread_id}
                   ORDER BY sent_at ASC
                   LIMIT {limit}"#
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_message)?;
        let mut messages: Vec<Message> = rows
            .map(|r| r.map_err(AppError::Database))
            .collect::<AppResult<Vec<_>>>()?;

        // Подгрузить вложения
        for msg in &mut messages {
            msg.attachments = self.get_attachments_for_message(msg.id)?;
        }

        Ok(messages)
    }

    pub fn mark_message_read(&self, message_id: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE messages SET is_read = 1 WHERE id = ?1",
            params![message_id],
        )?;
        Ok(())
    }

    /// Поиск сообщений по тексту (простой LIKE, FTS можно добавить позже).
    pub fn search_messages(&self, query: &str, limit: Option<i64>) -> AppResult<Vec<Message>> {
        let pattern = format!("%{query}%");
        let limit = limit.unwrap_or(50);
        let mut stmt = self.conn.prepare(&format!(
            r#"SELECT id, conversation_id, account_id, message_id, in_reply_to,
                      from_email, from_name, subject, body_text, body_html,
                      sent_at, is_outgoing, is_read, raw_headers, thread_id
               FROM messages
               WHERE body_text LIKE ? OR subject LIKE ?
               ORDER BY sent_at DESC
               LIMIT {limit}"#
        ))?;
        let rows = stmt.query_map(params![pattern, pattern], row_to_message)?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }
}

fn row_to_message(row: &rusqlite::Row<'_>) -> SqlResult<Message> {
    use chrono::DateTime;
    let sent_at_str: String = row.get(10)?;
    let sent_at = DateTime::parse_from_rfc3339(&sent_at_str)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(Message {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        account_id: row.get(2)?,
        message_id: row.get(3)?,
        in_reply_to: row.get(4)?,
        from_email: row.get(5)?,
        from_name: row.get(6)?,
        subject: row.get(7)?,
        body_text: row.get(8)?,
        body_html: row.get(9)?,
        sent_at,
        is_outgoing: row.get(10 + 1)?,
        is_read: row.get(10 + 2)?,
        raw_headers: row.get(10 + 3)?,
        thread_id: row.get(10 + 4)?,
        attachments: vec![],
    })
}

// ─── InsertMessage DTO ────────────────────────────────────────────────────────

/// DTO для вставки нового сообщения в БД (не содержит id и attachments).
#[derive(Debug)]
pub struct InsertMessage {
    pub conversation_id: i64,
    pub thread_id: i64,
    pub account_id: i64,
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub from_email: String,
    pub from_name: Option<String>,
    pub subject: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub is_outgoing: bool,
    pub is_read: bool,
    pub raw_headers: Option<String>,
}

// ─── Attachments ──────────────────────────────────────────────────────────────

impl Database {
    pub fn insert_attachment(
        &self,
        message_db_id: i64,
        filename: Option<&str>,
        mime_type: Option<&str>,
        size_bytes: Option<i64>,
        local_path: Option<&str>,
    ) -> AppResult<i64> {
        self.conn.execute(
            r#"INSERT INTO attachments (message_id, filename, mime_type, size_bytes, local_path)
               VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![message_db_id, filename, mime_type, size_bytes, local_path],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_attachments_for_message(&self, message_db_id: i64) -> AppResult<Vec<Attachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_id, filename, mime_type, size_bytes, local_path
             FROM attachments WHERE message_id = ?1",
        )?;
        let rows = stmt.query_map(params![message_db_id], |row| {
            Ok(Attachment {
                id: row.get(0)?,
                message_id: row.get(1)?,
                filename: row.get(2)?,
                mime_type: row.get(3)?,
                size_bytes: row.get(4)?,
                local_path: row.get(5)?,
            })
        })?;
        rows.map(|r| r.map_err(AppError::Database)).collect()
    }
}

// ─── Thread lookup (для JWZ) ──────────────────────────────────────────────────

impl Database {
    /// Найти conversation_id по Message-ID (для построения тредов).
    pub fn find_conversation_by_message_id(&self, message_id: &str) -> AppResult<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT conversation_id FROM messages WHERE message_id = ?1 LIMIT 1",
            params![message_id],
            |r| r.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    pub fn find_conversation_and_thread_by_message_id(&self, message_id: &str) -> AppResult<Option<(i64, i64)>> {
        let result = self.conn.query_row(
            "SELECT conversation_id, thread_id FROM messages WHERE message_id = ?1 LIMIT 1",
            params![message_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match result {
            Ok(tuple) => Ok(Some(tuple)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    pub fn find_channel_conversation(&self, contact_id: i64) -> AppResult<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT c.id FROM conversations c 
             JOIN conversation_members cm ON c.id = cm.conversation_id
             WHERE c.kind = 'channel' AND cm.contact_id = ?1 LIMIT 1",
            params![contact_id],
            |r| r.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }
}

// ─── Additional helpers ───────────────────────────────────────────────────────

impl Database {
    /// Set or update the subject hint on a conversation.
    pub fn set_conversation_subject_hint(
        &self,
        conversation_id: i64,
        subject: &str,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE conversations SET subject_hint = ?1 WHERE id = ?2 AND subject_hint IS NULL",
            params![subject, conversation_id],
        )?;
        Ok(())
    }

    /// Look up the subject of a message by its RFC Message-ID string.
    pub fn get_subject_for_message_id(&self, message_id: &str) -> AppResult<Option<String>> {
        let result = self.conn.query_row(
            "SELECT subject FROM messages WHERE message_id = ?1 LIMIT 1",
            params![message_id],
            |r| r.get::<_, Option<String>>(0),
        );
        match result {
            Ok(s) => Ok(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e)),
        }
    }

    /// Reconstruct the References chain for a message: fetch all message_ids
    /// in the same conversation that were sent before the given message,
    /// ordered by sent_at ASC.
    pub fn get_references_for_message_id(&self, message_id: &str) -> AppResult<Vec<String>> {
        // Find the conversation and sent_at of the given message.
        let result = self.conn.query_row(
            "SELECT conversation_id, sent_at FROM messages WHERE message_id = ?1 LIMIT 1",
            params![message_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        );

        let (conv_id, sent_at_str) = match result {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(vec![]),
            Err(e) => return Err(AppError::Database(e)),
        };

        let mut stmt = self.conn.prepare(
            r#"SELECT message_id FROM messages
               WHERE conversation_id = ?1
                 AND sent_at <= ?2
                 AND message_id != ?3
               ORDER BY sent_at ASC"#,
        )?;

        let rows = stmt.query_map(params![conv_id, sent_at_str, message_id], |r| {
            r.get::<_, String>(0)
        })?;

        let refs: Vec<String> = rows.filter_map(|r| r.ok()).collect();

        Ok(refs)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db() -> Database {
        Database::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn test_schema_creates_ok() {
        let db = make_db();
        let accounts = db.list_accounts().unwrap();
        assert!(accounts.is_empty());
    }

    #[test]
    fn test_upsert_contact() {
        let db = make_db();
        let id1 = db
            .upsert_contact("alice@example.com", Some("Alice"))
            .unwrap();
        let id2 = db.upsert_contact("alice@example.com", None).unwrap();
        assert_eq!(id1, id2, "upsert should return same id");

        let contact = db
            .get_contact_by_email("alice@example.com")
            .unwrap()
            .unwrap();
        assert_eq!(contact.display_name, Some("Alice".to_string()));
    }

    #[test]
    fn test_create_conversation_and_members() {
        let db = make_db();
        let alice_id = db
            .upsert_contact("alice@example.com", Some("Alice"))
            .unwrap();
        let bob_id = db.upsert_contact("bob@example.com", Some("Bob")).unwrap();

        let conv_id = db.create_conversation(&ConversationKind::Direct).unwrap();
        db.add_conversation_member(conv_id, alice_id, &MemberRole::To)
            .unwrap();
        db.add_conversation_member(conv_id, bob_id, &MemberRole::To)
            .unwrap();

        let members = db.get_conversation_members(conv_id).unwrap();
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn test_message_exists() {
        let db = make_db();
        let alice_id = db.upsert_contact("alice@example.com", None).unwrap();
        let bob_id = db.upsert_contact("bob@example.com", None).unwrap();

        // Сначала нужен account
        let acc = NewAccount {
            email: "bob@example.com".to_string(),
            display_name: None,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            password: "secret".to_string(),
        };
        let acc_id = db.insert_account(&acc, "keychain-ref-1").unwrap();

        let conv_id = db.create_conversation(&ConversationKind::Direct).unwrap();
        db.add_conversation_member(conv_id, alice_id, &MemberRole::To)
            .unwrap();
        db.add_conversation_member(conv_id, bob_id, &MemberRole::To)
            .unwrap();

        assert!(!db.message_exists("<msg1@example.com>").unwrap());

        let insert = InsertMessage {
            conversation_id: conv_id,
            account_id: acc_id,
            message_id: "<msg1@example.com>".to_string(),
            in_reply_to: None,
            from_email: "alice@example.com".to_string(),
            from_name: Some("Alice".to_string()),
            subject: Some("Hello".to_string()),
            body_text: Some("Hi Bob!".to_string()),
            body_html: None,
            sent_at: chrono::Utc::now(),
            is_outgoing: false,
            is_read: false,
            raw_headers: None,
        };
        db.insert_message(&insert).unwrap();
        assert!(db.message_exists("<msg1@example.com>").unwrap());
    }
}
