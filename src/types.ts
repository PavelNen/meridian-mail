/**
 * Shared TypeScript types mirroring the Rust backend models.
 * These types are intended for use across the Meridian Mail frontend
 * to ensure strong typing when calling Tauri commands and handling
 * data returned from the backend.
 */

export type ISODateString = string;
export type HexUUID = string;

/* ─── Accounts ─────────────────────────────────────────────────────────────── */

export interface Account {
  id: number;
  email: string;
  displayName?: string | null;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  keychainRef: string;
}

export interface NewAccountInput {
  email: string;
  displayName?: string | null;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  password: string;
}

export interface MacosAccount {
  name: string;
  email: string;
}

/* ─── Contacts ────────────────────────────────────────────────────────────── */

export interface Contact {
  id: number;
  email: string;
  displayName?: string | null;
  avatarUrl?: string | null;
  lastSeen?: ISODateString | null;
}

/* ─── Conversations ──────────────────────────────────────────────────────── */

export type ConversationKind = "direct" | "group";
export type MemberRole = "to" | "cc" | "bcc";

export interface ConversationMember {
  contact: Contact;
  role: MemberRole;
}

export interface Conversation {
  id: number;
  kind: ConversationKind;
  subjectHint?: string | null;
  lastMessageAt?: ISODateString | null;
  unreadCount: number;
  members: ConversationMember[];
  lastMessagePreview?: string | null;
}

export interface ConversationListItem {
  id: number;
  kind: ConversationKind;
  displayName: string;
  avatarLetters: string;
  lastMessagePreview?: string | null;
  lastMessageAt?: ISODateString | null;
  unreadCount: number;
  members: ConversationMember[];
}

/* ─── Messages & Attachments ─────────────────────────────────────────────── */

export interface Attachment {
  id: number;
  messageId: number;
  filename?: string | null;
  mimeType?: string | null;
  sizeBytes?: number | null;
  localPath?: string | null;
}

export interface Message {
  id: number;
  conversationId: number;
  accountId: number;
  messageId: string;
  inReplyTo?: string | null;
  fromEmail: string;
  fromName?: string | null;
  subject?: string | null;
  bodyText?: string | null;
  bodyHtml?: string | null;
  sentAt: ISODateString;
  isOutgoing: boolean;
  isRead: boolean;
  rawHeaders?: string | null;
  attachments: Attachment[];
}

/* ─── Send / Compose ─────────────────────────────────────────────────────── */

export interface Recipient {
  email: string;
  name?: string | null;
}

export interface SendMessagePayload {
  to: Recipient[];
  cc?: Recipient[];
  bcc?: Recipient[];
  subject?: string | null;
  body: string;
  inReplyToMessageId?: string | null;
}

export interface SendMessageResult {
  messageId: string;
}

/* ─── Sync / Status ─────────────────────────────────────────────────────── */

export interface SyncReport {
  newMessages: number;
  skipped: number;
  errors: number;
}

export interface SyncStatus {
  accountId: number;
  isSyncing: boolean;
  lastSyncAt?: ISODateString | null;
  error?: string | null;
}

/* ─── Utility Types ─────────────────────────────────────────────────────── */

export interface PaginatedMessagesResponse {
  conversationId: number;
  messages: Message[];
  nextBeforeId?: number;
}

export type AccountId = number;
export type ConversationId = number;
export type MessageId = number;
