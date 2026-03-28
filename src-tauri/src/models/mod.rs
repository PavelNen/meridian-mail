use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Account ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub keychain_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAccount {
    pub email: String,
    pub display_name: Option<String>,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub password: String, // только для создания, сразу уходит в Keychain
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacosAccount {
    pub name: String,
    pub email: String,
}

// ─── Contact ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub last_seen: Option<DateTime<Utc>>,
}

// ─── Conversation ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKind {
    Direct,
    Group,
    Channel,
}

impl ConversationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConversationKind::Direct => "direct",
            ConversationKind::Group => "group",
            ConversationKind::Channel => "channel",
        }
    }
}

impl TryFrom<&str> for ConversationKind {
    type Error = crate::error::AppError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "direct" => Ok(ConversationKind::Direct),
            "group" => Ok(ConversationKind::Group),
            "channel" => Ok(ConversationKind::Channel),
            other => Err(crate::error::AppError::InvalidData(format!(
                "unknown conversation kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub kind: ConversationKind,
    pub subject_hint: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
    /// Участники диалога (заполняется джойном)
    pub members: Vec<ConversationMember>,
    /// Последнее сообщение (превью)
    pub last_message_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMember {
    pub contact: Contact,
    pub role: MemberRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    To,
    Cc,
    Bcc,
}

impl MemberRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberRole::To => "to",
            MemberRole::Cc => "cc",
            MemberRole::Bcc => "bcc",
        }
    }
}

impl TryFrom<&str> for MemberRole {
    type Error = crate::error::AppError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "to" => Ok(MemberRole::To),
            "cc" => Ok(MemberRole::Cc),
            "bcc" => Ok(MemberRole::Bcc),
            other => Err(crate::error::AppError::InvalidData(format!(
                "unknown member role: {other}"
            ))),
        }
    }
}

// ─── Message ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: i64,
    pub account_id: i64,
    /// RFC 2822 Message-ID заголовок
    pub message_id: String,
    /// Message-ID родительского письма
    pub in_reply_to: Option<String>,
    /// Thread reference
    pub thread_id: i64,
    pub from_email: String,
    pub from_name: Option<String>,
    pub subject: Option<String>,
    /// Очищенный plain text (без цитат и подписей)
    pub body_text: Option<String>,
    /// Оригинальный HTML (для fallback)
    pub body_html: Option<String>,
    pub sent_at: DateTime<Utc>,
    pub is_outgoing: bool,
    pub is_read: bool,
    /// JSON с полными заголовками
    pub raw_headers: Option<String>,
    /// Вложения (заполняется отдельным запросом)
    pub attachments: Vec<Attachment>,
}

// ─── Attachment ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    pub message_id: i64,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub local_path: Option<String>,
}

// ─── Thread ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub conversation_id: i64,
    pub subject: String,
    pub normalized_subject: String,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadListItem {
    pub id: i64,
    pub conversation_id: i64,
    pub subject: String,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
}

// ─── DTO для UI ───────────────────────────────────────────────────────────────

/// Краткая информация о диалоге для списка
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListItem {
    pub id: i64,
    pub kind: ConversationKind,
    pub display_name: String,
    pub avatar_letters: String,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub unread_count: i64,
    pub members: Vec<ConversationMember>,
}

impl ConversationListItem {
    pub fn from_conversation(conv: &Conversation, my_email: &str) -> Self {
        let display_name = conversation_display_name(conv, my_email);
        let avatar_letters = make_avatar_letters(&display_name);
        Self {
            id: conv.id,
            kind: conv.kind.clone(),
            display_name,
            avatar_letters,
            last_message_preview: conv.last_message_preview.clone(),
            last_message_at: conv.last_message_at,
            unread_count: conv.unread_count,
            members: conv.members.clone(),
        }
    }
}

fn conversation_display_name(conv: &Conversation, my_email: &str) -> String {
    let others: Vec<&ConversationMember> = conv
        .members
        .iter()
        .filter(|m| m.contact.email != my_email)
        .collect();

    if others.is_empty() {
        return "Saved Messages".to_string();
    }

    if others.len() == 1 {
        let c = &others[0].contact;
        return c.display_name.clone().unwrap_or_else(|| c.email.clone());
    }

    // Группа
    others
        .iter()
        .map(|m| {
            m.contact
                .display_name
                .clone()
                .unwrap_or_else(|| m.contact.email.clone())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn make_avatar_letters(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.len() {
        0 => "?".to_string(),
        1 => words[0]
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string()),
        _ => {
            let first = words[0].chars().next().unwrap_or('?');
            let last = words[words.len() - 1].chars().next().unwrap_or('?');
            format!("{}{}", first.to_uppercase(), last.to_uppercase())
        }
    }
}

// ─── Credentials (не хранится в БД, только в Keychain) ───────────────────────

#[derive(Debug, Clone)]
pub struct Credentials {
    pub account_id: i64,
    pub username: String,
    pub password: String,
}

// ─── Sync state ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub account_id: i64,
    pub is_syncing: bool,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}
