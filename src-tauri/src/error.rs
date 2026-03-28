use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IMAP error: {0}")]
    Imap(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("MIME parse error: {0}")]
    MimeParse(String),

    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Account not found: {0}")]
    AccountNotFound(i64),

    #[error("Conversation not found: {0}")]
    ConversationNotFound(i64),

    #[error("Message not found: {0}")]
    MessageNotFound(i64),

    #[error("Authentication failed for {email}: {reason}")]
    AuthFailed { email: String, reason: String },

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Not initialized")]
    NotInitialized,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<native_tls::Error> for AppError {
    fn from(e: native_tls::Error) -> Self {
        AppError::Tls(e.to_string())
    }
}

impl From<lettre::transport::smtp::Error> for AppError {
    fn from(e: lettre::transport::smtp::Error) -> Self {
        AppError::Smtp(e.to_string())
    }
}

impl From<lettre::error::Error> for AppError {
    fn from(e: lettre::error::Error) -> Self {
        AppError::Smtp(e.to_string())
    }
}

/// Конвертация в строку для Tauri команд (команды возвращают Result<T, String>)
impl From<AppError> for String {
    fn from(e: AppError) -> Self {
        e.to_string()
    }
}

pub type AppResult<T> = Result<T, AppError>;
