use std::sync::Arc;

use async_imap::{Client, Session};
use futures::StreamExt;
use mail_parser::{Address, Message as MailMessage, MessageParser, MimeHeaders, PartType};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_native_tls::TlsStream;


use crate::error::{AppError, AppResult};
use crate::models::{Account, Credentials};

// ─── Types ────────────────────────────────────────────────────────────────────

type ImapSession = Session<TlsStream<TcpStream>>;

/// Parsed envelope data extracted from a raw email.
#[derive(Debug)]
pub struct ParsedEmail {
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from_email: String,
    pub from_name: Option<String>,
    pub to: Vec<(String, Option<String>)>,
    pub cc: Vec<(String, Option<String>)>,
    pub subject: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub raw_headers: String,
    pub attachments: Vec<ParsedAttachment>,
    /// Whether this message has a List-Unsubscribe header (newsletter detection).
    pub is_newsletter: bool,
}

#[derive(Debug)]
pub struct ParsedAttachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub size_bytes: usize,
    pub data: Vec<u8>,
}

// ─── ImapClient ───────────────────────────────────────────────────────────────

pub struct ImapClient {
    account: Account,
    session: Arc<Mutex<Option<ImapSession>>>,
}

impl ImapClient {
    pub fn new(account: Account) -> Self {
        Self {
            account,
            session: Arc::new(Mutex::new(None)),
        }
    }

    // ── Connection ────────────────────────────────────────────────────────────

    /// Connect and authenticate. Stores the session internally.
    pub async fn connect(&self, credentials: &Credentials) -> AppResult<()> {
        let session = Self::open_session(&self.account, credentials).await?;
        let mut guard = self.session.lock().await;
        *guard = Some(session);
        Ok(())
    }

    /// Disconnect gracefully.
    pub async fn disconnect(&self) -> AppResult<()> {
        let mut guard = self.session.lock().await;
        if let Some(mut session) = guard.take() {
            session
                .logout()
                .await
                .map_err(|e| AppError::Imap(e.to_string()))?;
        }
        Ok(())
    }

    /// Open a brand-new authenticated IMAP session.
    async fn open_session(account: &Account, credentials: &Credentials) -> AppResult<ImapSession> {
        let addr = format!("{}:{}", account.imap_host, account.imap_port);

        let tcp = TcpStream::connect(&addr)
            .await
            .map_err(|e| AppError::Connection(format!("TCP connect to {addr}: {e}")))?;

        let native_connector =
            native_tls::TlsConnector::new().map_err(|e| AppError::Tls(e.to_string()))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_connector);
        let tls = tls_connector
            .connect(&account.imap_host, tcp)
            .await
            .map_err(|e| AppError::Tls(e.to_string()))?;

        let client = Client::new(tls);
        let session = client
            .login(&credentials.username, &credentials.password)
            .await
            .map_err(|(e, _)| AppError::AuthFailed {
                email: credentials.username.clone(),
                reason: e.to_string(),
            })?;

        Ok(session)
    }

    // ── Capability check ──────────────────────────────────────────────────────

    pub async fn capabilities(&self) -> AppResult<Vec<String>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        let caps = session
            .capabilities()
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        Ok(caps.iter().map(|c| format!("{c:?}")).collect())
    }

    // ── Folder listing ────────────────────────────────────────────────────────

    pub async fn list_folders(&self) -> AppResult<Vec<ImapFolder>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        let mut stream = session
            .list(Some(""), Some("*"))
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            let f = item.map_err(|e| AppError::Imap(e.to_string()))?;
            result.push(ImapFolder {
                name: f.name().to_string(),
                delimiter: f.delimiter().map(|d| d.to_string()),
                attributes: f.attributes().iter().map(|a| format!("{a:?}")).collect(),
            });
        }

        Ok(result)
    }

    // ── Sync ──────────────────────────────────────────────────────────────────

    /// Select a mailbox (e.g. "INBOX") and return its metadata.
    pub async fn select_inbox(&self) -> AppResult<MailboxInfo> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        let mailbox = session
            .select("INBOX")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        Ok(MailboxInfo {
            exists: mailbox.exists,
            recent: mailbox.recent,
            uid_validity: mailbox.uid_validity,
            uid_next: mailbox.uid_next,
        })
    }

    /// Enter IDLE state and wait for server-side updates (e.g. new messages).
    /// This will block until an update is received or the keepalive timeout (29 min) is reached.
    pub async fn wait_for_updates(&self) -> AppResult<()> {
        let mut guard = self.session.lock().await;
        // We MUST take ownership to use .idle()
        let mut session = guard.take().ok_or(AppError::NotInitialized)?;

        // Entering IDLE requires a mailbox to be selected.
        if let Err(e) = session.select("INBOX").await {
            *guard = Some(session);
            return Err(AppError::Imap(e.to_string()));
        }

        let mut idle = session.idle();

        // In async-imap 0.11, wait() returns (Future, StopSource).
        let (future, _stop) = idle.wait();
        
        // Wait for an event
        let res = future.await;
        
        // Get the session back regardless of success/failure
        let session_back = idle.done().await.map_err(|e| AppError::Imap(e.to_string()))?;
        *guard = Some(session_back);
        
        res.map_err(|e| AppError::Imap(e.to_string()))?;
        Ok(())
    }

    /// Fetch the last `count` messages from INBOX by sequence number.
    /// Returns raw RFC 5322 bytes for each message together with its UID.
    pub async fn fetch_recent(&self, count: u32) -> AppResult<Vec<(u32, Vec<u8>)>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        let mailbox = session
            .select("INBOX")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let exists = mailbox.exists;
        if exists == 0 {
            return Ok(vec![]);
        }

        let start = if exists > count {
            exists - count + 1
        } else {
            1
        };
        let seq_set = format!("{start}:{exists}");

        let mut stream = session
            .fetch(&seq_set, "(UID RFC822)")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            let msg = item.map_err(|e| AppError::Imap(e.to_string()))?;
            if let Some(body) = msg.body() {
                let uid = msg.uid.unwrap_or(0);
                result.push((uid, body.to_vec()));
            }
        }

        Ok(result)
    }

    /// Fetch messages by UID range (for incremental sync).
    pub async fn fetch_since_uid(&self, since_uid: u32) -> AppResult<Vec<(u32, Vec<u8>)>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        session
            .select("INBOX")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let uid_set = format!("{since_uid}:*");

        let mut stream = session
            .uid_fetch(&uid_set, "(UID RFC822)")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            let msg = item.map_err(|e| AppError::Imap(e.to_string()))?;
            if let Some(body) = msg.body() {
                let uid = msg.uid.unwrap_or(0);
                result.push((uid, body.to_vec()));
            }
        }

        Ok(result)
    }

    /// Mark a message as deleted on the IMAP server by searching for its Message-ID.
    pub async fn delete_by_message_id(&self, message_id: &str) -> AppResult<()> {
        self.delete_messages_batch(&[message_id]).await
    }

    /// Delete multiple messages in a single IMAP session: one STORE + one EXPUNGE.
    pub async fn delete_messages_batch(&self, message_ids: &[&str]) -> AppResult<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        session
            .select("INBOX")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut all_uids: std::collections::HashSet<u32> = std::collections::HashSet::new();

        for message_id in message_ids {
            let safe_id = message_id.trim_matches(|c| c == '<' || c == '>');

            let query = format!("HEADER Message-ID \"{}\"", message_id);
            let uids = session.uid_search(&query).await.unwrap_or_default();

            if !uids.is_empty() {
                all_uids.extend(uids);
                continue;
            }

            // Fallback: search without angle brackets
            let fallback_query = format!("HEADER Message-ID \"{}\"", safe_id);
            let uids = session.uid_search(&fallback_query).await.unwrap_or_default();

            if !uids.is_empty() {
                all_uids.extend(uids);
                continue;
            }

            // Last resort: scan headers manually
            use futures::StreamExt;
            if let Ok(mut stream) = session
                .fetch("1:*", "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
                .await
            {
                while let Some(item) = stream.next().await {
                    if let Ok(msg) = item {
                        if let Some(header) = msg.header() {
                            if String::from_utf8_lossy(header).contains(safe_id) {
                                if let Some(uid) = msg.uid {
                                    all_uids.insert(uid);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if all_uids.is_empty() {
            return Ok(());
        }

        let uid_str = all_uids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");

        // Mark all collected UIDs as \Deleted in one round-trip.
        {
            use futures::StreamExt;
            let mut stream = Box::pin(
                session
                    .uid_store(&uid_str, "+FLAGS (\\Deleted)")
                    .await
                    .map_err(|e| AppError::Imap(e.to_string()))?,
            );
            while let Some(res) = stream.next().await {
                if let Err(e) = res {
                    log::warn!("[imap] uid_store error: {e}");
                }
            }
        }

        // One EXPUNGE for all marked messages.
        {
            use futures::StreamExt;
            let mut stream = Box::pin(
                session
                    .expunge()
                    .await
                    .map_err(|e| AppError::Imap(e.to_string()))?,
            );
            while let Some(res) = stream.next().await {
                if let Err(e) = res {
                    log::warn!("[imap] expunge error: {e}");
                }
            }
        }

        Ok(())
    }


    /// Fetch only headers (BODY[HEADER]) for a sequence range — much faster for
    /// initial index building.
    pub async fn fetch_headers(&self, seq_set: &str) -> AppResult<Vec<(u32, Vec<u8>)>> {
        let mut guard = self.session.lock().await;
        let session = guard.as_mut().ok_or(AppError::NotInitialized)?;

        session
            .select("INBOX")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut stream = session
            .fetch(seq_set, "(UID BODY[HEADER])")
            .await
            .map_err(|e| AppError::Imap(e.to_string()))?;

        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            let msg = item.map_err(|e| AppError::Imap(e.to_string()))?;
            if let Some(header) = msg.header() {
                let uid = msg.uid.unwrap_or(0);
                result.push((uid, header.to_vec()));
            }
        }

        Ok(result)
    }

    // ── IDLE ──────────────────────────────────────────────────────────────────

    /// Start IMAP IDLE on a *separate* connection and call `on_new_mail` whenever
    /// the server sends a notification. This function blocks until `stop_rx` fires.
    ///
    /// Caller should spawn this in a dedicated Tokio task.
    pub async fn idle_loop(
        account: Account,
        credentials: Credentials,
        on_new_mail: impl Fn() + Send + Sync + 'static,
        mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> AppResult<()> {
        let on_new_mail = std::sync::Arc::new(on_new_mail);
        loop {
            // Re-open a fresh session for each IDLE cycle (some servers close
            // the connection after the IDLE timeout).
            let session = match Self::open_session(&account, &credentials).await {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("IDLE: failed to connect, retrying in 30s: {e}");
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => continue,
                        _ = &mut stop_rx => break,
                    }
                }
            };

            let mut session = session;
            if let Err(e) = session.select("INBOX").await {
                log::warn!("IDLE: INBOX select failed: {e}");
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => continue,
                    _ = &mut stop_rx => break,
                }
            }

            // async-imap 0.11 IDLE API: idle() returns a Handle struct.
            // Handle::wait() returns (idle_future, StopSource) — we drive the
            // future and keep the StopSource alive so IDLE stays active.
            let mut idle_handle = session.idle();
            let (idle_future, _stop_source) = idle_handle.wait();
            let on_new_mail = std::sync::Arc::clone(&on_new_mail);

            // IDLE timeout: most servers impose 29 min, we renew every 20 min.
            let idle_timeout = std::time::Duration::from_secs(20 * 60);

            let idle_fut = async move {
                let result = tokio::time::timeout(idle_timeout, idle_future).await;
                match result {
                    Ok(Ok(_response)) => {
                        log::debug!("IDLE: server notification received");
                        on_new_mail();
                    }
                    Ok(Err(e)) => {
                        log::warn!("IDLE wait error: {e}");
                    }
                    Err(_) => {
                        // Timeout — renew IDLE cycle
                        log::debug!("IDLE: timeout, renewing");
                    }
                }
            };

            tokio::select! {
                _ = idle_fut => {}
                _ = &mut stop_rx => {
                    log::info!("IDLE loop stopped");
                    break;
                }
            }
        }

        Ok(())
    }
}

// ─── MIME parsing ─────────────────────────────────────────────────────────────

/// Parse raw RFC 5322 bytes into a structured `ParsedEmail`.
pub fn parse_email(raw: &[u8]) -> AppResult<ParsedEmail> {
    let parser = MessageParser::default();
    let msg = parser
        .parse(raw)
        .ok_or_else(|| AppError::MimeParse("failed to parse message".to_string()))?;

    // ── Message-ID ────────────────────────────────────────────────────────────
    let message_id = msg
        .message_id()
        .map(|s| format!("<{s}>"))
        .unwrap_or_else(|| format!("<generated-{}>", uuid::Uuid::new_v4()));

    // ── In-Reply-To ───────────────────────────────────────────────────────────
    let in_reply_to = msg.in_reply_to().as_text().map(|s| {
        if s.starts_with('<') {
            s.to_string()
        } else {
            format!("<{s}>")
        }
    });

    // ── References ────────────────────────────────────────────────────────────
    let references: Vec<String> = msg
        .references()
        .as_text_list()
        .map(|list| {
            list.iter()
                .map(|r| {
                    if r.starts_with('<') {
                        r.to_string()
                    } else {
                        format!("<{r}>")
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // ── From ──────────────────────────────────────────────────────────────────
    let (from_email, from_name) = extract_first_addr_opt(msg.from());

    // ── To / CC ───────────────────────────────────────────────────────────────
    let to = extract_addr_list_opt(msg.to());
    let cc = extract_addr_list_opt(msg.cc());

    // ── Subject ───────────────────────────────────────────────────────────────
    let subject = msg.subject().map(|s| s.to_string());

    // ── Date ──────────────────────────────────────────────────────────────────
    let sent_at = msg
        .date()
        .map(|d| {
            chrono::DateTime::from_timestamp(d.to_timestamp(), 0).unwrap_or_else(chrono::Utc::now)
        })
        .unwrap_or_else(chrono::Utc::now);

    // ── Body ──────────────────────────────────────────────────────────────────
    let body_text = msg.body_text(0).map(|t| strip_quotes_and_signature(&t));
    let body_html = msg.body_html(0).map(|h| h.to_string());

    // ── Attachments ───────────────────────────────────────────────────────────
    let attachments = extract_attachments(&msg);

    // ── Raw headers ───────────────────────────────────────────────────────────
    let raw_headers = extract_raw_headers(raw);

    // ── Newsletter detection ──────────────────────────────────────────────────
    let is_newsletter = raw_headers
        .to_ascii_lowercase()
        .contains("list-unsubscribe");

    Ok(ParsedEmail {
        message_id,
        in_reply_to,
        references,
        from_email,
        from_name,
        to,
        cc,
        subject,
        body_text,
        body_html,
        sent_at,
        raw_headers,
        attachments,
        is_newsletter,
    })
}

// ─── Address helpers ──────────────────────────────────────────────────────────

fn extract_first_addr_opt(addr: Option<&Address<'_>>) -> (String, Option<String>) {
    match addr {
        Some(Address::List(list)) => {
            if let Some(a) = list.first() {
                return (
                    a.address
                        .as_deref()
                        .unwrap_or("unknown@unknown")
                        .to_string(),
                    a.name.as_deref().map(|s| s.to_string()),
                );
            }
        }
        Some(Address::Group(groups)) => {
            for group in groups {
                if let Some(a) = group.addresses.first() {
                    return (
                        a.address
                            .as_deref()
                            .unwrap_or("unknown@unknown")
                            .to_string(),
                        a.name.as_deref().map(|s| s.to_string()),
                    );
                }
            }
        }
        _ => {}
    }
    ("unknown@unknown".to_string(), None)
}

fn extract_addr_list_opt(addr: Option<&Address<'_>>) -> Vec<(String, Option<String>)> {
    let mut result = Vec::new();
    match addr {
        Some(Address::List(list)) => {
            for a in list {
                if let Some(email) = a.address.as_deref() {
                    result.push((email.to_string(), a.name.as_deref().map(|s| s.to_string())));
                }
            }
        }
        Some(Address::Group(groups)) => {
            for group in groups {
                for a in &group.addresses {
                    if let Some(email) = a.address.as_deref() {
                        result.push((email.to_string(), a.name.as_deref().map(|s| s.to_string())));
                    }
                }
            }
        }
        _ => {}
    }
    result
}

// ─── Attachment helpers ───────────────────────────────────────────────────────

fn extract_attachments(msg: &MailMessage<'_>) -> Vec<ParsedAttachment> {
    let mut attachments = Vec::new();

    for part in msg.attachments() {
        let mime_type = part
            .content_type()
            .map(|ct| {
                let ctype = ct.ctype();
                let subtype = ct.subtype().unwrap_or("octet-stream");
                format!("{ctype}/{subtype}")
            })
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let filename = part.attachment_name().map(|s| s.to_string());

        let data: Vec<u8> = match &part.body {
            PartType::Binary(bytes) | PartType::InlineBinary(bytes) => bytes.to_vec(),
            PartType::Text(text) => text.as_bytes().to_vec(),
            _ => continue,
        };

        attachments.push(ParsedAttachment {
            filename,
            mime_type,
            size_bytes: data.len(),
            data,
        });
    }

    attachments
}

// ─── Raw header extraction ────────────────────────────────────────────────────

fn extract_raw_headers(raw: &[u8]) -> String {
    // Headers end at the first blank line (\r\n\r\n or \n\n).
    let separator = if let Some(pos) = find_subsequence(raw, b"\r\n\r\n") {
        pos
    } else if let Some(pos) = find_subsequence(raw, b"\n\n") {
        pos
    } else {
        raw.len()
    };
    String::from_utf8_lossy(&raw[..separator]).to_string()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ─── Quote & signature stripping ─────────────────────────────────────────────

/// Remove quoted reply content and email signatures from a plain-text body.
///
/// Strategy (per spec §10):
///   1. Lines beginning with `>` → quoted, remove.
///   2. Common "On ... wrote:" patterns → start of quote block, remove from here.
///   3. Signature delimiter `-- ` (RFC 3676) → remove from here.
///   4. Common separators: `---`, `___`, repeated `-` (8+).
pub fn strip_quotes_and_signature(text: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static RE_ON_WROTE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?mi)^(On .{0,200}wrote:|Le .{0,200}a écrit :|Am .{0,200}schrieb:|El .{0,200}escribió:|-{3,}[ \t]*(original message|forwarded message)[ \t]*-{3,})",
        )
        .unwrap()
    });

    static RE_SIG_DASHES: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^--[ \t]*$").unwrap());

    static RE_HARD_SEP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[_\-]{8,}[ \t]*$").unwrap());

    // Find the earliest cut point.
    let mut cut = text.len();

    if let Some(m) = RE_ON_WROTE.find(text) {
        cut = cut.min(m.start());
    }
    if let Some(m) = RE_SIG_DASHES.find(text) {
        cut = cut.min(m.start());
    }
    if let Some(m) = RE_HARD_SEP.find(text) {
        cut = cut.min(m.start());
    }

    let trimmed = &text[..cut];

    // Remove individual `>` quoted lines.
    let lines: Vec<&str> = trimmed
        .lines()
        .filter(|line| !line.trim_start().starts_with('>'))
        .collect();

    // Trim trailing blank lines.
    let joined = lines.join("\n");
    joined.trim_end().to_string()
}

// ─── Supporting structs ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImapFolder {
    pub name: String,
    pub delimiter: Option<String>,
    pub attributes: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MailboxInfo {
    pub exists: u32,
    pub recent: u32,
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_quotes_gmail_style() {
        let body = "Hey, how are you?\n\nOn Mon, 1 Jan 2024, Alice wrote:\n> I'm fine\n> thanks";
        let result = strip_quotes_and_signature(body);
        assert_eq!(result.trim(), "Hey, how are you?");
    }

    #[test]
    fn test_strip_signature() {
        let body = "See you tomorrow!\n\n-- \nBob\nbob@example.com";
        let result = strip_quotes_and_signature(body);
        assert_eq!(result.trim(), "See you tomorrow!");
    }

    #[test]
    fn test_strip_inline_quotes() {
        let body = "OK sounds good.\n> Original message\n> blah blah";
        let result = strip_quotes_and_signature(body);
        assert_eq!(result.trim(), "OK sounds good.");
    }

    #[test]
    fn test_no_strip_when_clean() {
        let body = "Just a plain message with no quotes.";
        let result = strip_quotes_and_signature(body);
        assert_eq!(result, body);
    }

    #[test]
    fn test_parse_minimal_email() {
        let raw = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hello\r\nMessage-ID: <test@example.com>\r\n\r\nHello Bob!";
        let parsed = parse_email(raw).unwrap();
        assert_eq!(parsed.from_email, "alice@example.com");
        assert_eq!(parsed.subject.as_deref(), Some("Hello"));
        assert!(parsed
            .body_text
            .as_deref()
            .unwrap_or("")
            .contains("Hello Bob"));
    }
}
