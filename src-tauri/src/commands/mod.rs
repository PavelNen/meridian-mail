use std::sync::Arc;
use tokio::sync::Mutex;

use tauri::State;

use crate::db::Database;
use crate::error::AppResult;
use crate::imap::{parse_email, ImapClient, ImapFolder, MailboxInfo};
use crate::models::*;
use crate::smtp::{OutgoingMessage, SmtpSender};

// ─── App state ────────────────────────────────────────────────────────────────

pub struct AppState {
    pub db: Arc<Mutex<Database>>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
        }
    }
}

// ─── Account commands ─────────────────────────────────────────────────────────

/// Add a new email account. Credentials are stored in macOS Keychain.
#[tauri::command]
pub async fn add_account(
    state: State<'_, AppState>,
    account: NewAccount,
) -> Result<Account, String> {
    let db = state.db.lock().await;

    // Store password in Keychain and get a reference key back.
    let keychain_ref = crate::keychain::save_password(&account.email, &account.password)
        .map_err(|e| e.to_string())?;

    let id = db
        .insert_account(&account, &keychain_ref)
        .map_err(|e| e.to_string())?;
    let acc = db.get_account(id).map_err(|e| e.to_string())?;
    Ok(acc)
}

/// List all configured accounts.
#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    let db = state.db.lock().await;
    db.list_accounts().map_err(|e| e.to_string())
}

/// Remove an account by ID.
#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, account_id: i64) -> Result<(), String> {
    let db = state.db.lock().await;
    let acc = db.get_account(account_id).map_err(|e| e.to_string())?;
    crate::keychain::delete_password(&acc.email).ok(); // best-effort
    db.delete_account(account_id).map_err(|e| e.to_string())
}

/// List accounts from macOS Mail via AppleScript.
#[tauri::command]
pub async fn list_macos_mail_accounts() -> Result<Vec<MacosAccount>, String> {
    let script = r#"
        tell application "Mail"
            set accList to {}
            repeat with acc in accounts
                set end of accList to (name of acc & "|" & (email addresses of acc as string))
            end repeat
            return accList
        end tell
    "#;

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut accounts = Vec::new();
    // osa returns "item 1, item 2"
    for part in stdout.trim().split(", ") {
        if let Some((name, email)) = part.split_once('|') {
            accounts.push(MacosAccount {
                name: name.to_string(),
                email: email.to_string(),
            });
        }
    }
    Ok(accounts)
}

// ─── IMAP commands ────────────────────────────────────────────────────────────

/// Test IMAP connectivity for an account.
#[tauri::command]
pub async fn test_imap_connection(
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<Vec<ImapFolder>, String> {
    let (acc, creds) = load_account_and_creds(&state, account_id).await?;
    let client = ImapClient::new(acc);
    client.connect(&creds).await.map_err(|e| e.to_string())?;
    let folders = client.list_folders().await.map_err(|e| e.to_string())?;
    client.disconnect().await.ok();
    Ok(folders)
}

/// Get INBOX metadata (exists count, uid_validity, etc.)
#[tauri::command]
pub async fn get_inbox_info(
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<MailboxInfo, String> {
    let (acc, creds) = load_account_and_creds(&state, account_id).await?;
    let client = ImapClient::new(acc);
    client.connect(&creds).await.map_err(|e| e.to_string())?;
    let info = client.select_inbox().await.map_err(|e| e.to_string())?;
    client.disconnect().await.ok();
    Ok(info)
}

/// Perform a full sync of the last N messages from INBOX into the local DB.
/// Returns the number of new messages stored.
#[tauri::command]
pub async fn sync_inbox(
    state: State<'_, AppState>,
    account_id: i64,
    fetch_count: Option<u32>,
) -> Result<SyncReport, String> {
    let count = fetch_count.unwrap_or(500);
    let (acc, creds) = load_account_and_creds(&state, account_id).await?;

    let client = ImapClient::new(acc.clone());
    client.connect(&creds).await.map_err(|e| e.to_string())?;

    // Fetch messages WITHOUT holding the DB lock.
    let raw_messages = client
        .fetch_recent(count)
        .await
        .map_err(|e| e.to_string())?;

    let report = {
        let db = state.db.lock().await;
        perform_sync_internal(&db, raw_messages, account_id, &acc.email)
            .map_err(|e| e.to_string())?
    };

    client.disconnect().await.ok();
    Ok(report)
}

// ─── Internal Sync Logic ───────────────────────────────────────────────────

fn perform_sync_internal(
    db: &crate::db::Database,
    raw_messages: Vec<(u32, Vec<u8>)>,
    account_id: i64,
    my_email: &str,
) -> AppResult<SyncReport> {
    let mut new_messages = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for (_uid, raw) in raw_messages {
        match parse_email(&raw) {
            Ok(parsed) => {
                if db.message_exists(&parsed.message_id).unwrap_or(false) {
                    skipped += 1;
                    continue;
                }

                let (conv_id, thread_id) = match resolve_conversation_and_thread(db, &parsed, my_email, account_id) {
                    Ok(res) => res,
                    Err(e) => {
                        log::warn!(
                            "Failed to resolve conversation for {}: {e}",
                            parsed.message_id
                        );
                        errors += 1;
                        continue;
                    }
                };

                // Determine directionality.
                let is_outgoing = parsed.from_email.to_lowercase() == my_email.to_lowercase();
                let is_read = is_outgoing;

                let raw_headers_json = serde_json::to_string(&parsed.raw_headers).ok();

                let insert = crate::db::InsertMessage {
                    conversation_id: conv_id,
                    thread_id,
                    account_id,
                    message_id: parsed.message_id.clone(),
                    in_reply_to: parsed.in_reply_to.clone(),
                    from_email: parsed.from_email.clone(),
                    from_name: parsed.from_name.clone(),
                    subject: parsed.subject.clone(),
                    body_text: parsed.body_text.clone(),
                    body_html: parsed.body_html.clone(),
                    sent_at: parsed.sent_at,
                    is_outgoing,
                    is_read,
                    raw_headers: raw_headers_json,
                };

                match db.insert_message(&insert) {
                    Ok(msg_db_id) => {
                        let _ = db.update_conversation_last_message(
                            conv_id,
                            &parsed.sent_at,
                            parsed.body_text.as_deref(),
                        );

                        if !is_outgoing && !is_read {
                            let _ = db.increment_unread(conv_id);
                            let _ = db.increment_thread_unread(thread_id);
                        }

                        if let Some(ref subj) = parsed.subject {
                            let _ = db.set_conversation_subject_hint(conv_id, subj);
                        }
                        
                        let _ = db.update_thread_last_message(thread_id, &parsed.sent_at);

                        for att in &parsed.attachments {
                            let _ = db.insert_attachment(
                                msg_db_id,
                                att.filename.as_deref(),
                                Some(&att.mime_type),
                                Some(att.size_bytes as i64),
                                None,
                            );
                        }

                        new_messages += 1;
                    }
                    Err(e) => {
                        log::error!("Failed to insert message {}: {e}", parsed.message_id);
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to parse email: {e}");
                errors += 1;
            }
        }
    }

    Ok(SyncReport {
        new_messages,
        skipped,
        errors,
    })
}

/// Start a background polling task that syncs every 60 seconds.
/// More reliable than IMAP IDLE which has session-management complexity.
#[tauri::command]
pub async fn start_idle_sync(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<(), String> {
    use tauri::Emitter;

    // Validate that the account exists and credentials are accessible right now.
    let (acc, _) = load_account_and_creds(&state, account_id).await?;
    let db_arc = state.db.clone();

    tokio::spawn(async move {
        log::info!("Starting polling sync for account {}", acc.email);
        // Initial short delay to let the app finish loading.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        loop {
            // Re-read password from Keychain on every iteration so that
            // re-added accounts (new password) are picked up automatically,
            // and stale workers self-terminate when the account is removed.
            let password = match crate::keychain::get_password(&acc.email) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("Poll: Keychain error for {} — stopping worker: {e}", acc.email);
                    break;
                }
            };

            // Verify the account still exists in DB (not deleted by user).
            let account_still_exists = {
                let db = db_arc.lock().await;
                db.get_account(account_id).is_ok()
            };
            if !account_still_exists {
                log::info!("Poll: account {} removed — stopping worker", acc.email);
                break;
            }

            let creds = crate::models::Credentials {
                account_id,
                username: acc.email.clone(),
                password,
            };

            log::debug!("Poll: syncing inbox for {}", acc.email);

            let client = ImapClient::new(acc.clone());
            match client.connect(&creds).await {
                Err(e) => {
                    log::error!("Poll: IMAP connect failed for {}: {e}", acc.email);
                }
                Ok(()) => {
                    match client.fetch_recent(50).await {
                        Ok(raw_messages) => {
                            let db = db_arc.lock().await;
                            match perform_sync_internal(&db, raw_messages, account_id, &acc.email) {
                                Ok(report) if report.new_messages > 0 => {
                                    log::info!(
                                        "Poll: {} new messages for {}",
                                        report.new_messages,
                                        acc.email
                                    );
                                    let _ = app_handle.emit("db-updated", ());
                                }
                                Err(e) => {
                                    log::error!("Poll: sync error for {}: {e}", acc.email);
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            log::error!("Poll: fetch error for {}: {e}", acc.email);
                        }
                    }
                    client.disconnect().await.ok();
                }
            }

            // Poll every 60 seconds.
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    Ok(())
}

// ─── Conversation commands ────────────────────────────────────────────────────

/// List all conversations for the main screen, sorted by last message date.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
    account_id: i64,
) -> Result<Vec<ConversationListItem>, String> {
    let db = state.db.lock().await;
    let acc = db.get_account(account_id).map_err(|e| e.to_string())?;
    let conversations = db.list_conversations(account_id).map_err(|e| e.to_string())?;
    let items = conversations
        .iter()
        .map(|c| ConversationListItem::from_conversation(c, &acc.email))
        .collect();
    Ok(items)
}

/// Mark all messages in a conversation as read.
#[tauri::command]
pub async fn mark_conversation_read(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<(), String> {
    let db = state.db.lock().await;
    db.mark_conversation_read(conversation_id)
        .map_err(|e| e.to_string())
}

/// Delete a single message by ID.
#[tauri::command]
pub async fn delete_message(
    state: State<'_, AppState>,
    message_id: i64,
) -> Result<(), String> {
    let msg = {
        let db = state.db.lock().await;
        match db.get_message_by_id(message_id) {
            Ok(m) => m,
            Err(_) => return Ok(()), // Already deleted or doesn't exist
        }
    };

    if let Ok((acc, creds)) = load_account_and_creds(&state, msg.account_id).await {
        let client = ImapClient::new(acc);
        if client.connect(&creds).await.is_ok() {
            if let Err(e) = client.delete_by_message_id(&msg.message_id).await {
                println!("DEBUG [delete_message]: failed to delete from IMAP server: {}", e);
            }
            let _ = client.disconnect().await;
        }
    }

    let db = state.db.lock().await;
    db.delete_message(message_id).map_err(|e| e.to_string())
}

/// Delete an entire conversation and all its messages.
#[tauri::command]
pub async fn delete_conversation(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> Result<(), String> {
    // Fetch only message_ids — no body needed for deletion.
    let (message_ids, account_id) = {
        let db = state.db.lock().await;
        let ids = db.get_message_ids_for_conversation(conversation_id).unwrap_or_default();
        // Derive account_id from first message for IMAP credentials.
        let acc_id = db
            .get_messages_for_conversation(conversation_id, Some(1), None)
            .ok()
            .and_then(|msgs| msgs.into_iter().next())
            .map(|m| m.account_id);
        (ids, acc_id)
    };

    // Delete all messages from IMAP server in a single session.
    if !message_ids.is_empty() {
        if let Some(aid) = account_id {
            if let Ok((acc, creds)) = load_account_and_creds(&state, aid).await {
                let client = ImapClient::new(acc);
                if client.connect(&creds).await.is_ok() {
                    let refs: Vec<&str> = message_ids.iter().map(|s| s.as_str()).collect();
                    let _ = client.delete_messages_batch(&refs).await;
                    let _ = client.disconnect().await;
                }
            }
        }
    }

    let db = state.db.lock().await;
    db.delete_conversation(conversation_id)
        .map_err(|e| e.to_string())
}

// ─── Message commands ─────────────────────────────────────────────────────────

/// Load messages for a conversation (paginated, oldest-first).
#[tauri::command]
pub async fn get_messages(
    state: State<'_, AppState>,
    conversation_id: i64,
    limit: Option<i64>,
    before_id: Option<i64>,
) -> Result<Vec<Message>, String> {
    let db = state.db.lock().await;
    db.get_messages_for_conversation(conversation_id, limit, before_id)
        .map_err(|e| e.to_string())
}

/// Full-text search across message bodies and subjects.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let db = state.db.lock().await;
    db.search_messages(&query, limit).map_err(|e| e.to_string())
}

// ─── Contact commands ─────────────────────────────────────────────────────────

/// List all known contacts.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<Contact>, String> {
    let db = state.db.lock().await;
    db.list_contacts().map_err(|e| e.to_string())
}

/// Fuzzy-search contacts by name or email address.
#[tauri::command]
pub async fn search_contacts(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<Contact>, String> {
    let db = state.db.lock().await;
    db.search_contacts(&query).map_err(|e| e.to_string())
}

// ─── Send command ─────────────────────────────────────────────────────────────

/// Send a new message or reply. Stores a copy in the local DB immediately
/// (optimistic UI) and also saves to IMAP Sent folder.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    account_id: i64,
    payload: SendMessagePayload,
) -> Result<Message, String> {
    let (acc, creds) = load_account_and_creds(&state, account_id).await?;

    // Build subject: preserve "Re:" for replies, auto-generate for new threads.
    let subject = if let Some(ref s) = payload.subject {
        s.clone()
    } else if let Some(ref irt) = payload.in_reply_to_message_id {
        // Look up original subject.
        let db = state.db.lock().await;
        let orig_subject = db
            .get_subject_for_message_id(irt)
            .ok()
            .flatten()
            .unwrap_or_else(|| "Re: (no subject)".to_string());
        drop(db);
        if orig_subject.to_lowercase().starts_with("re:") {
            orig_subject
        } else {
            format!("Re: {orig_subject}")
        }
    } else {
        "(no subject)".to_string()
    };

    // Gather References chain if this is a reply.
    let references = if let Some(ref irt) = payload.in_reply_to_message_id {
        let db = state.db.lock().await;
        let mut refs = db.get_references_for_message_id(irt).unwrap_or_default();
        refs.push(irt.clone());
        drop(db);
        refs
    } else {
        vec![]
    };

    let outgoing = OutgoingMessage {
        from_email: acc.email.clone(),
        from_name: acc.display_name.clone(),
        to: payload.to.clone(),
        cc: payload.cc.clone().unwrap_or_default(),
        bcc: payload.bcc.clone().unwrap_or_default(),
        subject: subject.clone(),
        body_text: payload.body.clone(),
        in_reply_to: payload.in_reply_to_message_id.clone(),
        references,
        attachments: vec![], // file attachments handled separately in Phase 3
    };

    // Send via SMTP.
    let sender = SmtpSender::new(acc.clone());
    let send_result = sender
        .send(&outgoing, &creds)
        .await
        .map_err(|e| e.to_string())?;

    // Determine or create conversation for the sent message.
    let db = state.db.lock().await;

    // Resolve all participants (to + cc) for conversation lookup.
    let mut all_participants = outgoing.to.clone();
    all_participants.extend(outgoing.cc.iter().cloned());

    let (conv_id, thread_id) = if let Some(ref irt) = payload.in_reply_to_message_id {
        // Reply: find existing conversation.
        db.find_conversation_and_thread_by_message_id(irt)
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let cid = create_conversation_for_participants(&db, &all_participants, &acc.email)
                    .unwrap_or(1);
                let tid = db.create_thread(cid, &subject, &normalize_subject(Some(&subject))).unwrap_or(1);
                (cid, tid)
            })
    } else {
        let cid = create_conversation_for_participants(&db, &all_participants, &acc.email)
            .map_err(|e| e.to_string())?;
        let tid = db.create_thread(cid, &subject, &normalize_subject(Some(&subject))).map_err(|e| e.to_string())?;
        (cid, tid)
    };

    let now = chrono::Utc::now();

    let insert = crate::db::InsertMessage {
        conversation_id: conv_id,
        thread_id,
        account_id,
        message_id: send_result.message_id.clone(),
        in_reply_to: payload.in_reply_to_message_id.clone(),
        from_email: acc.email.clone(),
        from_name: acc.display_name.clone(),
        subject: Some(subject.clone()),
        body_text: Some(payload.body.clone()),
        body_html: None,
        sent_at: now,
        is_outgoing: true,
        is_read: true,
        raw_headers: None,
    };

    let msg_db_id = db.insert_message(&insert).map_err(|e| e.to_string())?;
    let _ = db.update_conversation_last_message(conv_id, &now, Some(&payload.body));
    let _ = db.update_thread_last_message(thread_id, &now);

    let mut msg = db
        .get_message_by_id(msg_db_id)
        .map_err(|e| format!("Failed to retrieve sent message from DB: {e}"))?;

    msg.attachments = vec![];
    Ok(msg)
}

// ─── Payload structs ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SendMessagePayload {
    pub to: Vec<(String, Option<String>)>,
    pub cc: Option<Vec<(String, Option<String>)>>,
    pub bcc: Option<Vec<(String, Option<String>)>>,
    /// Subject override. If None, derived from in_reply_to or auto-generated.
    pub subject: Option<String>,
    pub body: String,
    /// Message-ID of the message being replied to (for threading).
    pub in_reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncReport {
    pub new_messages: u32,
    pub skipped: u32,
    pub errors: u32,
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Load an account from the DB and retrieve its credentials from Keychain.
async fn load_account_and_creds(
    state: &State<'_, AppState>,
    account_id: i64,
) -> Result<(Account, Credentials), String> {
    let db = state.db.lock().await;
    let acc = db
        .get_account(account_id)
        .map_err(|e| format!("Account {account_id} not found: {e}"))?;
    drop(db);

    let password = crate::keychain::get_password(&acc.email)
        .map_err(|e| format!("Keychain error for {}: {e}", acc.email))?;

    let creds = Credentials {
        account_id,
        username: acc.email.clone(),
        password,
    };

    Ok((acc, creds))
}

fn normalize_subject(subject: Option<&str>) -> String {
    let mut s = subject.unwrap_or("(no subject)").to_string();
    loop {
        let lower = s.to_lowercase();
        if lower.starts_with("re:") {
            s = s[3..].trim().to_string();
        } else if lower.starts_with("re ") {
            s = s[3..].trim().to_string();
        } else if lower.starts_with("fwd:") {
            s = s[4..].trim().to_string();
        } else if lower.starts_with("fw:") {
            s = s[3..].trim().to_string();
        } else if lower.starts_with("aw:") {
            s = s[3..].trim().to_string();
        } else {
            break;
        }
    }
    if s.is_empty() {
        return "(no subject)".to_string();
    }
    s
}

/// Determine the correct conversation for an incoming parsed email.
/// Implements the thread-resolution + contact-grouping strategy from spec §9.
fn resolve_conversation_and_thread(
    db: &Database,
    parsed: &crate::imap::ParsedEmail,
    my_email: &str,
    _account_id: i64,
) -> AppResult<(i64, i64)> {
    let subject = parsed.subject.as_deref().unwrap_or("(no subject)");
    let normalized = normalize_subject(parsed.subject.as_deref());

    if parsed.is_newsletter {
        let sender_id = db.upsert_contact(&parsed.from_email, parsed.from_name.as_deref())?;
        let conv_id = match db.find_channel_conversation(sender_id)? {
            Some(id) => id,
            None => {
                let id = db.create_conversation(&ConversationKind::Channel)?;
                db.add_conversation_member(id, sender_id, &MemberRole::To)?;
                id
            }
        };
        let thread_id = db.create_thread(conv_id, subject, &normalized)?;
        return Ok((conv_id, thread_id));
    }

    // 1. Thread resolution: check In-Reply-To and References.
    if let Some(ref irt) = parsed.in_reply_to {
        if let Ok(Some((conv_id, thread_id))) = db.find_conversation_and_thread_by_message_id(irt) {
            // Extend group if new CC participants appeared.
            extend_conversation_members(db, conv_id, parsed)?;
            return Ok((conv_id, thread_id));
        }
    }
    for ref_id in &parsed.references {
        if let Ok(Some((conv_id, thread_id))) = db.find_conversation_and_thread_by_message_id(ref_id) {
            extend_conversation_members(db, conv_id, parsed)?;
            return Ok((conv_id, thread_id));
        }
    }

    // 2. Collect all participants (from + to + cc), excluding ourselves.
    let mut participants: Vec<(String, Option<String>)> = vec![];

    if parsed.from_email.to_lowercase() != my_email.to_lowercase() {
        participants.push((parsed.from_email.clone(), parsed.from_name.clone()));
    }
    for (email, name) in &parsed.to {
        if email.to_lowercase() != my_email.to_lowercase() {
            participants.push((email.clone(), name.clone()));
        }
    }
    for (email, name) in &parsed.cc {
        if email.to_lowercase() != my_email.to_lowercase() {
            participants.push((email.clone(), name.clone()));
        }
    }

    // Also include ourselves so the conversation has both sides.
    let my_contact_id = db.upsert_contact(my_email, None)?;

    // Upsert all participant contacts.
    let mut contact_ids: Vec<i64> = vec![my_contact_id];
    for (email, name) in &participants {
        let cid = db.upsert_contact(email, name.as_deref())?;
        if !contact_ids.contains(&cid) {
            contact_ids.push(cid);
        }
    }
    contact_ids.sort();

    // 3. Direct vs Group.
    let conv_id = if contact_ids.len() == 2 {
        let other_id = *contact_ids.iter().find(|&&id| id != my_contact_id).unwrap();
        if let Some(c_id) = db.find_direct_conversation(my_contact_id, other_id)? {
            c_id
        } else {
            let c_id = db.create_conversation(&ConversationKind::Direct)?;
            db.add_conversation_member(c_id, my_contact_id, &MemberRole::To)?;
            db.add_conversation_member(c_id, other_id, &MemberRole::To)?;
            c_id
        }
    } else {
        if let Some(c_id) = db.find_group_conversation(&contact_ids)? {
            c_id
        } else {
            let c_id = db.create_conversation(&ConversationKind::Group)?;
            db.add_conversation_member(c_id, my_contact_id, &MemberRole::To)?;
            for (email, name) in &participants {
                let cid = db.upsert_contact(email, name.as_deref())?;
                let role = if parsed.cc.iter().any(|(e, _)| e == email) {
                    MemberRole::Cc
                } else {
                    MemberRole::To
                };
                db.add_conversation_member(c_id, cid, &role)?;
            }
            c_id
        }
    };

    let thread_id = db.create_thread(conv_id, subject, &normalized)?;
    Ok((conv_id, thread_id))
}

/// Add any newly-appearing CC participants to an existing conversation.
fn extend_conversation_members(
    db: &Database,
    conv_id: i64,
    parsed: &crate::imap::ParsedEmail,
) -> AppResult<()> {
    for (email, name) in &parsed.cc {
        let cid = db.upsert_contact(email, name.as_deref())?;
        // add_conversation_member uses INSERT OR IGNORE so duplicates are safe.
        db.add_conversation_member(conv_id, cid, &MemberRole::Cc)?;
    }
    Ok(())
}

/// Find or create a conversation for a given set of recipients when sending.
fn create_conversation_for_participants(
    db: &Database,
    participants: &[(String, Option<String>)],
    my_email: &str,
) -> AppResult<i64> {
    let my_id = db.upsert_contact(my_email, None)?;

    let mut contact_ids = vec![my_id];
    for (email, name) in participants {
        let cid = db.upsert_contact(email, name.as_deref())?;
        if !contact_ids.contains(&cid) {
            contact_ids.push(cid);
        }
    }
    contact_ids.sort();

    // Reuse existing conversation with the same participant set.
    if contact_ids.len() == 2 {
        let other_id = *contact_ids.iter().find(|&&id| id != my_id).unwrap();
        if let Some(existing) = db.find_direct_conversation(my_id, other_id)? {
            return Ok(existing);
        }
    } else if contact_ids.len() > 2 {
        if let Some(existing) = db.find_group_conversation(&contact_ids)? {
            return Ok(existing);
        }
    }

    let kind = if contact_ids.len() <= 2 {
        ConversationKind::Direct
    } else {
        ConversationKind::Group
    };

    let conv_id = db.create_conversation(&kind)?;
    db.add_conversation_member(conv_id, my_id, &MemberRole::To)?;
    for (email, name) in participants {
        let cid = db.upsert_contact(email, name.as_deref())?;
        db.add_conversation_member(conv_id, cid, &MemberRole::To)?;
    }

    Ok(conv_id)
}
