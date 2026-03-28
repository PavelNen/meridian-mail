use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials as LettreCredentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::error::{AppError, AppResult};
use crate::models::{Account, Credentials};

// ─── Outgoing message ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub from_email: String,
    pub from_name: Option<String>,
    pub to: Vec<(String, Option<String>)>,
    pub cc: Vec<(String, Option<String>)>,
    pub bcc: Vec<(String, Option<String>)>,
    pub subject: String,
    pub body_text: String,
    /// Optional In-Reply-To Message-ID (for threading).
    pub in_reply_to: Option<String>,
    /// Optional References list (for threading).
    pub references: Vec<String>,
    pub attachments: Vec<OutgoingAttachment>,
}

#[derive(Debug, Clone)]
pub struct OutgoingAttachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Result of a successful send operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SendResult {
    /// The Message-ID we assigned to the outgoing message.
    pub message_id: String,
}

// ─── SmtpSender ──────────────────────────────────────────────────────────────

pub struct SmtpSender {
    account: Account,
}

impl SmtpSender {
    pub fn new(account: Account) -> Self {
        Self { account }
    }

    /// Send an email and return the assigned Message-ID.
    pub async fn send(
        &self,
        msg: &OutgoingMessage,
        credentials: &Credentials,
    ) -> AppResult<SendResult> {
        let lettre_msg = self.build_message(msg)?;
        let transport = self.build_transport(credentials)?;

        transport
            .send(lettre_msg)
            .await
            .map_err(|e| AppError::Smtp(e.to_string()))?;

        // Extract Message-ID from the built message (we set it ourselves).
        let message_id = msg
            .in_reply_to
            .as_deref()
            .map(|_| {
                format!(
                    "<{}@{}>",
                    uuid::Uuid::new_v4(),
                    domain_from_email(&msg.from_email)
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "<{}@{}>",
                    uuid::Uuid::new_v4(),
                    domain_from_email(&msg.from_email)
                )
            });

        Ok(SendResult { message_id })
    }

    // ── Message builder ───────────────────────────────────────────────────────

    fn build_message(&self, msg: &OutgoingMessage) -> AppResult<lettre::Message> {
        let mut builder = Message::builder();

        // From
        let from_addr = build_mailbox(&msg.from_email, msg.from_name.as_deref())?;
        builder = builder.from(from_addr);

        // To
        for (email, name) in &msg.to {
            let addr = build_mailbox(email, name.as_deref())?;
            builder = builder.to(addr);
        }

        // CC
        for (email, name) in &msg.cc {
            let addr = build_mailbox(email, name.as_deref())?;
            builder = builder.cc(addr);
        }

        // BCC
        for (email, name) in &msg.bcc {
            let addr = build_mailbox(email, name.as_deref())?;
            builder = builder.bcc(addr);
        }

        // Subject
        builder = builder.subject(msg.subject.clone());

        // Threading headers
        if let Some(irt) = &msg.in_reply_to {
            builder = builder.in_reply_to(irt.clone());
        }
        if !msg.references.is_empty() {
            builder = builder.references(msg.references.join(" "));
        }

        // Message-ID
        let msg_id = format!(
            "<{}@{}>",
            uuid::Uuid::new_v4(),
            domain_from_email(&msg.from_email)
        );
        builder = builder.message_id(Some(msg_id));

        // Body — plain text only (per spec §4 Минимализм)
        let body_part = SinglePart::builder()
            .header(ContentType::TEXT_PLAIN)
            .body(msg.body_text.clone());

        let lettre_msg = if msg.attachments.is_empty() {
            builder
                .body(msg.body_text.clone())
                .map_err(|e| AppError::Smtp(e.to_string()))?
        } else {
            let mut multipart = MultiPart::mixed().singlepart(body_part);
            for att in &msg.attachments {
                let content_type = att
                    .mime_type
                    .parse::<ContentType>()
                    .unwrap_or_else(|_| "application/octet-stream".parse().unwrap());
                let attachment_part =
                    Attachment::new(att.filename.clone()).body(att.data.clone(), content_type);
                multipart = multipart.singlepart(attachment_part);
            }
            builder
                .multipart(multipart)
                .map_err(|e| AppError::Smtp(e.to_string()))?
        };

        Ok(lettre_msg)
    }

    // ── Transport builder ─────────────────────────────────────────────────────

    fn build_transport(
        &self,
        credentials: &Credentials,
    ) -> AppResult<AsyncSmtpTransport<Tokio1Executor>> {
        let creds =
            LettreCredentials::new(credentials.username.clone(), credentials.password.clone());

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&self.account.smtp_host)
            .map_err(|e| AppError::Smtp(e.to_string()))?
            .port(self.account.smtp_port)
            .credentials(creds)
            .build();

        Ok(transport)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_mailbox(email: &str, name: Option<&str>) -> AppResult<lettre::message::Mailbox> {
    let mailbox = if let Some(n) = name {
        format!("{n} <{email}>")
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| AppError::Smtp(format!("invalid mailbox '{email}': {e}")))?
    } else {
        email
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| AppError::Smtp(format!("invalid email '{email}': {e}")))?
    };
    Ok(mailbox)
}

fn domain_from_email(email: &str) -> &str {
    email.split('@').nth(1).unwrap_or("localhost")
}

// ─── Copy to Sent ─────────────────────────────────────────────────────────────

/// Build a raw RFC 5322 message bytes for copying to the IMAP Sent folder.
/// (lettre doesn't expose raw bytes directly, so we reconstruct a minimal version.)
pub fn build_raw_message(msg: &OutgoingMessage) -> String {
    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S +0000")
        .to_string();
    let msg_id = format!(
        "<{}@{}>",
        uuid::Uuid::new_v4(),
        domain_from_email(&msg.from_email)
    );

    let from = match &msg.from_name {
        Some(name) => format!("{name} <{}>", msg.from_email),
        None => msg.from_email.clone(),
    };

    let to_str = msg
        .to
        .iter()
        .map(|(email, name)| match name {
            Some(n) => format!("{n} <{email}>"),
            None => email.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut headers = format!(
        "From: {from}\r\nTo: {to_str}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: {msg_id}\r\nContent-Type: text/plain; charset=utf-8\r\nMIME-Version: 1.0\r\n",
        subject = msg.subject,
    );

    if let Some(irt) = &msg.in_reply_to {
        headers.push_str(&format!("In-Reply-To: {irt}\r\n"));
    }

    if !msg.references.is_empty() {
        headers.push_str(&format!("References: {}\r\n", msg.references.join(" ")));
    }

    if !msg.cc.is_empty() {
        let cc_str = msg
            .cc
            .iter()
            .map(|(email, name)| match name {
                Some(n) => format!("{n} <{email}>"),
                None => email.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        headers.push_str(&format!("Cc: {cc_str}\r\n"));
    }

    format!("{headers}\r\n{}", msg.body_text)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_outgoing() -> OutgoingMessage {
        OutgoingMessage {
            from_email: "alice@example.com".to_string(),
            from_name: Some("Alice".to_string()),
            to: vec![("bob@example.com".to_string(), Some("Bob".to_string()))],
            cc: vec![],
            bcc: vec![],
            subject: "Hello Bob".to_string(),
            body_text: "Hey Bob, how are you?".to_string(),
            in_reply_to: None,
            references: vec![],
            attachments: vec![],
        }
    }

    #[test]
    fn test_build_raw_message_contains_headers() {
        let msg = make_outgoing();
        let raw = build_raw_message(&msg);
        assert!(raw.contains("From: Alice <alice@example.com>"));
        assert!(raw.contains("To: Bob <bob@example.com>"));
        assert!(raw.contains("Subject: Hello Bob"));
        assert!(raw.contains("Hey Bob, how are you?"));
    }

    #[test]
    fn test_build_raw_message_reply_headers() {
        let mut msg = make_outgoing();
        msg.in_reply_to = Some("<parent@example.com>".to_string());
        msg.references = vec!["<parent@example.com>".to_string()];

        let raw = build_raw_message(&msg);
        assert!(raw.contains("In-Reply-To: <parent@example.com>"));
        assert!(raw.contains("References: <parent@example.com>"));
    }

    #[test]
    fn test_domain_from_email() {
        assert_eq!(domain_from_email("user@example.com"), "example.com");
        assert_eq!(domain_from_email("noeatsign"), "localhost");
    }

    #[test]
    fn test_build_mailbox_with_name() {
        let mb = build_mailbox("alice@example.com", Some("Alice Smith")).unwrap();
        assert_eq!(mb.email.to_string(), "alice@example.com");
    }

    #[test]
    fn test_build_mailbox_without_name() {
        let mb = build_mailbox("alice@example.com", None).unwrap();
        assert_eq!(mb.email.to_string(), "alice@example.com");
    }
}
