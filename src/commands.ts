import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  NewAccountInput,
  ConversationListItem,
  Conversation,
  Message,
  Contact,
  SyncReport,
  SendMessagePayload,
  Recipient,
  MacosAccount,
} from "./types";

/* ─── Wire-level (snake_case) types coming from Rust ──────────────────────── */

type RawAccount = {
  id: number;
  email: string;
  display_name?: string | null;
  imap_host: string;
  imap_port: number;
  smtp_host: string;
  smtp_port: number;
  keychain_ref: string;
};

type RawContact = {
  id: number;
  email: string;
  display_name?: string | null;
  avatar_url?: string | null;
  last_seen?: string | null;
};

type RawConversationMember = {
  contact: RawContact;
  role: "to" | "cc" | "bcc";
};

type RawConversation = {
  id: number;
  kind: "direct" | "group";
  subject_hint?: string | null;
  last_message_at?: string | null;
  unread_count: number;
  members: RawConversationMember[];
  last_message_preview?: string | null;
};

type RawConversationListItem = {
  id: number;
  kind: "direct" | "group";
  display_name: string;
  avatar_letters: string;
  last_message_preview?: string | null;
  last_message_at?: string | null;
  unread_count: number;
  members: RawConversationMember[];
};

type RawAttachment = {
  id: number;
  message_id: number;
  filename?: string | null;
  mime_type?: string | null;
  size_bytes?: number | null;
  local_path?: string | null;
};

type RawMessage = {
  id: number;
  conversation_id: number;
  account_id: number;
  message_id: string;
  in_reply_to?: string | null;
  from_email: string;
  from_name?: string | null;
  subject?: string | null;
  body_text?: string | null;
  body_html?: string | null;
  sent_at: string;
  is_outgoing: boolean;
  is_read: boolean;
  raw_headers?: string | null;
  attachments: RawAttachment[];
};

type RawSyncReport = {
  new_messages: number;
  skipped: number;
  errors: number;
};

/* ─── Mapping helpers (snake_case → camelCase) ───────────────────────────── */

const toUndefined = <T>(value: T | null | undefined): T | undefined =>
  value === null || value === undefined ? undefined : value;

const mapAccount = (raw: RawAccount): Account => ({
  id: raw.id,
  email: raw.email,
  displayName: toUndefined(raw.display_name),
  imapHost: raw.imap_host,
  imapPort: raw.imap_port,
  smtpHost: raw.smtp_host,
  smtpPort: raw.smtp_port,
  keychainRef: raw.keychain_ref,
});

const mapContact = (raw: RawContact): Contact => ({
  id: raw.id,
  email: raw.email,
  displayName: toUndefined(raw.display_name),
  avatarUrl: toUndefined(raw.avatar_url),
  lastSeen: toUndefined(raw.last_seen),
});

const mapConversationMember = (raw: RawConversationMember) => ({
  contact: mapContact(raw.contact),
  role: raw.role,
});

const mapConversation = (raw: RawConversation): Conversation => ({
  id: raw.id,
  kind: raw.kind,
  subjectHint: toUndefined(raw.subject_hint),
  lastMessageAt: toUndefined(raw.last_message_at),
  unreadCount: raw.unread_count,
  members: raw.members.map(mapConversationMember),
  lastMessagePreview: toUndefined(raw.last_message_preview),
});

const mapConversationListItem = (
  raw: RawConversationListItem,
): ConversationListItem => ({
  id: raw.id,
  kind: raw.kind,
  displayName: raw.display_name,
  avatarLetters: raw.avatar_letters,
  lastMessagePreview: toUndefined(raw.last_message_preview),
  lastMessageAt: toUndefined(raw.last_message_at),
  unreadCount: raw.unread_count,
  members: raw.members.map(mapConversationMember),
});

const mapAttachment = (raw: RawAttachment) => ({
  id: raw.id,
  messageId: raw.message_id,
  filename: toUndefined(raw.filename),
  mimeType: toUndefined(raw.mime_type),
  sizeBytes: toUndefined(raw.size_bytes ?? undefined),
  localPath: toUndefined(raw.local_path),
});

const mapMessage = (raw: RawMessage): Message => ({
  id: raw.id,
  conversationId: raw.conversation_id,
  accountId: raw.account_id,
  messageId: raw.message_id,
  inReplyTo: toUndefined(raw.in_reply_to),
  fromEmail: raw.from_email,
  fromName: toUndefined(raw.from_name),
  subject: toUndefined(raw.subject),
  bodyText: toUndefined(raw.body_text),
  bodyHtml: toUndefined(raw.body_html),
  sentAt: raw.sent_at,
  isOutgoing: raw.is_outgoing,
  isRead: raw.is_read,
  rawHeaders: toUndefined(raw.raw_headers),
  attachments: raw.attachments?.map(mapAttachment) ?? [],
});

const mapSyncReport = (raw: RawSyncReport): SyncReport => ({
  newMessages: raw.new_messages,
  skipped: raw.skipped,
  errors: raw.errors,
});

/* ─── Input transformers (camelCase → snake_case) ────────────────────────── */

const toWireNewAccount = (input: NewAccountInput) => ({
  email: input.email,
  display_name: input.displayName ?? null,
  imap_host: input.imapHost,
  imap_port: input.imapPort,
  smtp_host: input.smtpHost,
  smtp_port: input.smtpPort,
  password: input.password,
});

const toWireRecipients = (list?: Recipient[]) =>
  list?.map<[string, string | null]>((recipient) => [
    recipient.email,
    recipient.name ?? null,
  ]);

const toWireSendPayload = (payload: SendMessagePayload) => ({
  to: toWireRecipients(payload.to) ?? [],
  cc: payload.cc && payload.cc.length > 0 ? toWireRecipients(payload.cc) : undefined,
  bcc: payload.bcc && payload.bcc.length > 0 ? toWireRecipients(payload.bcc) : undefined,
  subject: payload.subject ?? undefined,
  body: payload.body,
  in_reply_to_message_id: payload.inReplyToMessageId ?? undefined,
});

/* ─── Public command helpers ─────────────────────────────────────────────── */

export const listAccounts = async (): Promise<Account[]> => {
  const accounts = await invoke<RawAccount[]>("list_accounts");
  return accounts.map(mapAccount);
};

export const addAccount = async (
  input: NewAccountInput,
): Promise<Account> => {
  const raw = await invoke<RawAccount>("add_account", {
    account: toWireNewAccount(input),
  });
  return mapAccount(raw);
};

export const removeAccount = async (accountId: number): Promise<void> => {
  await invoke("remove_account", { accountId });
};

export const listConversations = async (
  accountId: number,
): Promise<ConversationListItem[]> => {
  const raw = await invoke<RawConversationListItem[]>("list_conversations", {
    accountId,
  });
  return raw.map(mapConversationListItem);
};

export const fetchConversation = async (
  conversationId: number,
): Promise<Conversation> => {
  const raw = await invoke<RawConversation>("get_conversation", {
    conversationId,
  });
  return mapConversation(raw);
};

export const markConversationRead = async (
  conversationId: number,
): Promise<void> => {
  await invoke("mark_conversation_read", { conversationId });
};

export const getMessages = async (
  conversationId: number,
  options?: { limit?: number; beforeId?: number },
): Promise<Message[]> => {
  const payload: Record<string, unknown> = {
    conversationId,
  };
  if (options?.limit !== undefined) {
    payload.limit = options.limit;
  }
  if (options?.beforeId !== undefined) {
    payload.beforeId = options.beforeId;
  }

  const raw = await invoke<RawMessage[]>("get_messages", payload);
  return raw.map(mapMessage);
};

export const searchMessages = async (
  query: string,
  limit?: number,
): Promise<Message[]> => {
  const raw = await invoke<RawMessage[]>("search_messages", {
    query,
    limit,
  });
  return raw.map(mapMessage);
};

export const sendMessage = async (
  accountId: number,
  payload: SendMessagePayload,
): Promise<Message> => {
  const raw = await invoke<RawMessage>("send_message", {
    accountId,
    payload: toWireSendPayload(payload),
  });
  return mapMessage(raw);
};

export const syncInbox = async (
  accountId: number,
  fetchCount?: number,
): Promise<SyncReport> => {
  const raw = await invoke<RawSyncReport>("sync_inbox", {
    accountId,
    fetchCount,
  });
  return mapSyncReport(raw);
};

export const listContacts = async (): Promise<Contact[]> => {
  const raw = await invoke<RawContact[]>("list_contacts");
  return raw.map(mapContact);
};

export const searchContacts = async (
  query: string,
): Promise<Contact[]> => {
  const raw = await invoke<RawContact[]>("search_contacts", { query });
  return raw.map(mapContact);
};

export const startSyncWorker = async (accountId: number): Promise<void> => {
  return await invoke("start_idle_sync", { accountId });
};

export const listMacosMailAccounts = async (): Promise<MacosAccount[]> => {
  return await invoke<MacosAccount[]>("list_macos_mail_accounts");
};

export const deleteMessage = async (messageId: number): Promise<void> => {
  await invoke("delete_message", { messageId });
};

export const deleteConversation = async (conversationId: number): Promise<void> => {
  await invoke("delete_conversation", { conversationId });
};

/* Optional helper to expose all commands together */

export const meridianCommands = {
  listAccounts,
  addAccount,
  removeAccount,
  listConversations,
  fetchConversation,
  markConversationRead,
  getMessages,
  searchMessages,
  sendMessage,
  syncInbox,
  listContacts,
  searchContacts,
  startSyncWorker,
};
