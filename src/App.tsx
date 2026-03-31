import { useCallback, useEffect, useMemo, useState } from "react";
import { MessageItem } from "./components/MessageItem";
import { motion, AnimatePresence } from "framer-motion";
import { format } from "date-fns";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import { ScrollArea } from "@/components/ui/scroll-area";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { cn } from "@/lib/utils";
import {
  Mail,
  RefreshCw,
  User,
  Plus,
  SendHorizontal,
  Trash2,
  X,
  Loader2,
  Check,
  AlertCircle,
} from "lucide-react";
import {
  listAccounts,
  listConversations,
  getMessages,
  sendMessage,
  markConversationRead,
  syncInbox,
  startSyncWorker,
  deleteMessage,
  deleteConversation,
} from "./commands";
import type {
  Account,
  ConversationListItem,
  Message,
  SendMessagePayload,
} from "./types";

import { AccountSetup } from "./components/AccountSetup";
import { NewConversation } from "./components/NewConversation";

type LoadingState = "idle" | "loading" | "error";

const formatTimestamp = (value?: string | Date | null, fallback = "") => {
  if (!value) return fallback;
  const date = typeof value === "string" ? new Date(value) : value;
  if (Number.isNaN(date.getTime())) return fallback;
  return format(date, "d MMM yyyy, HH:mm");
};

// Запросить разрешение на уведомления при первом запуске
async function ensureNotificationPermission(): Promise<boolean> {
  let granted = await isPermissionGranted();
  if (!granted) {
    const permission = await requestPermission();
    granted = permission === "granted";
  }
  return granted;
}

async function notifyNewMessage(senderName: string) {
  if (!(await ensureNotificationPermission())) return;
  sendNotification({ title: "Meridian Mail", body: `Новое сообщение от ${senderName}` });
}

const formatTime = (value?: string | null, fallback = "") => {
  if (!value) return fallback;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return fallback;
  return format(date, "HH:mm");
};

export default function App() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<number | null>(
    null,
  );

  const [conversations, setConversations] = useState<ConversationListItem[]>(
    [],
  );
  const [conversationsState, setConversationsState] =
    useState<LoadingState>("idle");

  const [selectedConversationId, setSelectedConversationId] = useState<
    number | null
  >(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [messagesState, setMessagesState] = useState<LoadingState>("idle");

  const [draft, setDraft] = useState("");
  const [showDialogInfo, setShowDialogInfo] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const [showCompose, setShowCompose] = useState(false);
  const [initialLoading, setInitialLoading] = useState(true);

  const selectedAccount = useMemo(
    () => accounts.find((a) => a.id === selectedAccountId) ?? null,
    [accounts, selectedAccountId],
  );

  const selectedConversation = useMemo(
    () => conversations.find((c) => c.id === selectedConversationId) ?? null,
    [conversations, selectedConversationId],
  );

  const loadAccounts = useCallback(async () => {
    setInitialLoading(true);
    try {
      const list = await listAccounts();
      setAccounts(list);
      if (list.length > 0) {
        setShowSetup(false);
        setSelectedAccountId((current) => {
          const stillExists = current !== null && list.some((a) => a.id === current);
          return stillExists ? current : list[0].id;
        });
      } else {
        setSelectedAccountId(null);
        setConversations([]);
        setSelectedConversationId(null);
        setShowSetup(true);
      }
    } catch (error) {
      console.error("Failed to load accounts", error);
    } finally {
      setInitialLoading(false);
    }
  }, []);

  const loadConversations = useCallback(async (accountId: number) => {
    setConversationsState("loading");
    try {
      const list = await listConversations(accountId);
      setConversations(list);
      if (list.length > 0) {
        setSelectedConversationId((current) => {
          if (current && list.some((item) => item.id === current)) {
            return current;
          }
          return list[0].id;
        });
      } else {
        setSelectedConversationId(null);
      }
      setConversationsState("idle");
      return list;
    } catch (error) {
      setConversationsState("error");
      console.error("Failed to load conversations", error);
    }
  }, []);

  const loadMessages = useCallback(async (conversationId: number) => {
    setMessagesState("loading");
    try {
      const list = await getMessages(conversationId);
      setMessages(list);
      setMessagesState("idle");
    } catch (error) {
      setMessagesState("error");
      console.error("Failed to load messages", error);
    }
  }, []);

  const performSync = useCallback(
    async (accountId: number) => {
      setIsSyncing(true);
      setSyncError(null);
      try {
        await syncInbox(accountId, 200);
        await loadConversations(accountId);
      } catch (error) {
        setSyncError("Синхронизация не удалась — проверьте подключение.");
        console.error("Failed to sync inbox", error);
      } finally {
        setIsSyncing(false);
      }
    },
    [loadConversations],
  );

  useEffect(() => {
    loadAccounts();
  }, [loadAccounts]);

  useEffect(() => {
    if (selectedAccountId == null) {
      setConversations([]);
      setSelectedConversationId(null);
      return;
    }
    // Clear stale data immediately before loading the new account's conversations.
    setConversations([]);
    setSelectedConversationId(null);
    void loadConversations(selectedAccountId).then((list) => {
      // Auto-sync if inbox is empty — e.g. after first account setup.
      if ((list ?? []).length === 0) {
        void performSync(selectedAccountId);
      }
    });
  }, [selectedAccountId, loadConversations]);

  useEffect(() => {
    if (selectedConversationId == null) {
      setMessages([]);
      return;
    }
    void loadMessages(selectedConversationId);
    void markConversationRead(selectedConversationId)
      .then(() => {
        setConversations((prev) =>
          prev.map((c) => c.id === selectedConversationId ? { ...c, unreadCount: 0 } : c)
        );
      })
      .catch((error) => console.error("Failed to mark conversation read", error));
  }, [selectedConversationId, loadMessages]);

  useEffect(() => {
    const unlisten = listen("db-updated", () => {
      if (selectedAccountId) {
        const prevConvs = conversations;
        void loadConversations(selectedAccountId).then((next) => {
          if (!next) return;
          // Найти диалоги у которых вырос unreadCount
          for (const conv of next) {
            const prev = prevConvs.find((c) => c.id === conv.id);
            const isNew = !prev;
            const hasMoreUnread = prev && conv.unreadCount > prev.unreadCount;
            if ((isNew || hasMoreUnread) && conv.unreadCount > 0) {
              void notifyNewMessage(conv.displayName ?? conv.id.toString());
            }
          }
        });
      }
      if (selectedConversationId) {
        void loadMessages(selectedConversationId);
      }
    });

    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [
    selectedAccountId,
    selectedConversationId,
    conversations,
    loadConversations,
    loadMessages,
  ]);

  useEffect(() => {
    if (selectedAccountId) {
      void startSyncWorker(selectedAccountId).catch((err: unknown) => {
        const msg = String(err);
        if (msg.includes("Keychain") || msg.includes("secure storage") || msg.includes("No matching entry")) {
          setSyncError("Не удалось получить пароль — переподключите аккаунт.");
        } else {
          console.error("Failed to start sync worker", err);
        }
      });
    }
  }, [selectedAccountId]);

  // Запрашиваем разрешение на уведомления при старте
  useEffect(() => {
    void ensureNotificationPermission();
  }, []);

  const handleDeleteMessage = useCallback(async (messageId: number) => {
    try {
      await deleteMessage(messageId);
      setMessages((prev) => prev.filter((m) => m.id !== messageId));
    } catch (err) {
      console.error("Failed to delete message", err);
    }
  }, []);

  const handleDeleteConversation = useCallback((conversationId: number) => {
    const conv = conversations.find((c) => c.id === conversationId);
    if (!conv) return;

    // Немедленно убираем из списка и сбрасываем выбор
    setConversations((prev) => prev.filter((c) => c.id !== conversationId));
    if (selectedConversationId === conversationId) {
      setSelectedConversationId(null);
      setMessages([]);
    }

    // Показываем тост «Удаление...»
    setDeleteToasts((prev) => [
      ...prev,
      { id: conversationId, name: conv.displayName ?? "Диалог", status: "deleting" },
    ]);

    // Фоновое удаление на IMAP
    void deleteConversation(conversationId)
      .then(() => {
        setDeleteToasts((prev) =>
          prev.map((t) => (t.id === conversationId ? { ...t, status: "done" } : t))
        );
        setTimeout(() => {
          setDeleteToasts((prev) => prev.filter((t) => t.id !== conversationId));
        }, 1500);
      })
      .catch((err) => {
        console.error("Failed to delete conversation", err);
        // Возвращаем диалог обратно
        setConversations((prev) =>
          [...prev, conv].sort(
            (a, b) =>
              new Date(b.lastMessageAt ?? 0).getTime() -
              new Date(a.lastMessageAt ?? 0).getTime()
          )
        );
        setDeleteToasts((prev) =>
          prev.map((t) => (t.id === conversationId ? { ...t, status: "error" } : t))
        );
        setTimeout(() => {
          setDeleteToasts((prev) => prev.filter((t) => t.id !== conversationId));
        }, 3000);
      });
  }, [selectedConversationId, conversations]);

  // Conversation context menu state
  const [convContextMenu, setConvContextMenu] = useState<{ x: number; y: number; conversationId: number } | null>(null);

  // Тосты фонового удаления
  type DeleteToast = { id: number; name: string; status: "deleting" | "done" | "error" };
  const [deleteToasts, setDeleteToasts] = useState<DeleteToast[]>([]);

  useEffect(() => {
    if (!convContextMenu) return;
    const close = () => setConvContextMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [convContextMenu]);

  const handleSend = async () => {
    if (!selectedAccountId || !selectedConversation || !draft.trim()) {
      return;
    }

    const myEmail = selectedAccount?.email.toLowerCase();

    const payload: SendMessagePayload = {
      to: selectedConversation.members
        .filter((member) => member.role === "to" && member.contact.email.toLowerCase() !== myEmail)
        .map((member) => ({
          email: member.contact.email,
          name: member.contact.displayName ?? undefined,
        })),
      cc: selectedConversation.members
        .filter((member) => member.role === "cc" && member.contact.email.toLowerCase() !== myEmail)
        .map((member) => ({
          email: member.contact.email,
          name: member.contact.displayName ?? undefined,
        })),
      body: draft.trim(),
      subject: selectedConversation.lastMessagePreview ?? undefined,
      inReplyToMessageId: messages[messages.length - 1]?.messageId ?? undefined,
    };

    setSendError(null);
    try {
      const sent = await sendMessage(selectedAccountId, payload);
      setMessages((current) => [...current, sent]);
      setDraft("");
      await loadConversations(selectedAccountId);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      setSendError(msg);
      console.error("Failed to send message", error);
    }
  };

  const handleSelectConversation = (conversationId: number) => {
    setSelectedConversationId(conversationId);
    setShowDialogInfo(false);
  };

  const conversationHeader = useMemo(() => {
    if (!selectedConversation) return null;
    const memberEmails = selectedConversation.members
      .map((member) => member.contact.email)
      .filter((email, index, arr) => arr.indexOf(email) === index);
    return {
      displayName: selectedConversation.displayName ?? memberEmails.join(", "),
      participantsCount: memberEmails.length,
      lastMessageAt: selectedConversation.lastMessageAt,
    };
  }, [selectedConversation]);

  return (
    <div className="h-screen w-screen bg-background text-foreground overflow-hidden relative">
      <AnimatePresence>
        {showSetup && (
          <motion.div 
            key="setup"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/10 backdrop-blur-sm p-4"
            onClick={(e) => {
              if (e.target === e.currentTarget && accounts.length > 0) {
                setShowSetup(false);
              }
            }}
          >
            <div className="w-full max-w-md overflow-hidden rounded-2xl border bg-background shadow-2xl">
              <AccountSetup 
                onComplete={() => {
                  void loadAccounts();
                  setShowSetup(false);
                }}
                onAccountsChanged={() => {
                  void loadAccounts();
                }}
                onCancel={accounts.length > 0 ? () => setShowSetup(false) : undefined}
                selectedAccountId={selectedAccountId}
                onSelectAccount={setSelectedAccountId}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {showCompose && selectedAccountId && (
          <motion.div
            key="compose-modal"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 flex items-end justify-center sm:items-center bg-black/10 backdrop-blur-sm p-4"
            onClick={(e) => {
              if (e.target === e.currentTarget) setShowCompose(false);
            }}
          >
            <motion.div
              initial={{ opacity: 0, y: 16, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 16, scale: 0.97 }}
              transition={{ type: "spring", damping: 28, stiffness: 320 }}
              className="w-full max-w-md h-[420px] overflow-hidden rounded-2xl border bg-background shadow-2xl flex flex-col"
            >
              <NewConversation
                accountId={selectedAccountId}
                onClose={() => setShowCompose(false)}
                onSent={async (message) => {
                  setShowCompose(false);
                  await loadConversations(selectedAccountId);
                  setSelectedConversationId(message.conversationId);
                }}
              />
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {showDialogInfo && selectedConversation && conversationHeader && (
          <motion.div
            key="info-modal-wrapper"
            className="fixed inset-0 z-[60] flex items-start justify-center p-4 pt-[12vh] bg-black/40 backdrop-blur-md"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            onClick={(e) => {
              if (e.target === e.currentTarget) setShowDialogInfo(false);
            }}
          >
            <motion.div
              key="info-panel"
              initial={{ opacity: 0, scale: 0.95, y: 10 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.95, y: 10 }}
              transition={{ type: "spring", damping: 25, stiffness: 300 }}
              className="w-full max-w-sm bg-[hsl(240_10%_94%)] rounded-xl shadow-xl flex flex-col overflow-hidden max-h-[100dvh] md:max-h-[85vh] relative"
            >
              {/* Close button on grey background */}
              <div className="absolute right-3 top-3 z-20">
                <Button variant="ghost" size="icon" onClick={() => setShowDialogInfo(false)} className="h-9 w-9 text-muted-foreground rounded-full hover:bg-black/8 transition-colors">
                  <X className="h-5 w-5" />
                </Button>
              </div>

              {/* Avatar hero area – sits directly on grey */}
              <div className="flex flex-col items-center gap-3 pt-12 pb-5 px-6 text-center">
                <Avatar className="h-24 w-24">
                  {selectedConversation.members[0]?.contact?.avatarUrl ? (
                    <AvatarImage src={selectedConversation.members[0].contact.avatarUrl} />
                  ) : null}
                  <AvatarFallback className="bg-primary/10 text-primary text-3xl font-bold">
                    {selectedConversation.avatarLetters || "MM"}
                  </AvatarFallback>
                </Avatar>
                <div className="space-y-0.5">
                  <h3 className="text-xl font-bold tracking-tight">{conversationHeader.displayName}</h3>
                  <p className="text-sm text-muted-foreground">{conversationHeader.participantsCount} участников</p>
                </div>
              </div>

              {/* White cards scroll area */}
              <ScrollArea className="flex-1">
                <div className="flex flex-col min-h-full">

                  {/* Participants card – stretches to bottom */}
                  <div className="bg-background flex-1">
                    <div className="px-4 pt-3 pb-1.5">
                      <h4 className="text-[10px] font-bold uppercase tracking-widest text-primary">Участники</h4>
                    </div>
                    {selectedConversation.members.map((member, i) => (
                      <div key={i} className="flex items-center gap-3 px-4 py-2.5 hover:bg-muted/40 transition-colors cursor-pointer">
                        <Avatar className="h-10 w-10">
                          {member.contact.avatarUrl ? <AvatarImage src={member.contact.avatarUrl} /> : null}
                          <AvatarFallback className="bg-primary/10 text-primary text-xs font-bold">
                            {(member.contact.displayName || member.contact.email).slice(0, 2).toUpperCase()}
                          </AvatarFallback>
                        </Avatar>
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-semibold truncate">{member.contact.displayName || member.contact.email.split('@')[0]}</p>
                          <p className="text-[11px] text-muted-foreground truncate">{member.contact.email}</p>
                        </div>
                        <Badge variant="outline" className="text-[9px] uppercase font-bold tracking-wider shrink-0">{member.role}</Badge>
                      </div>
                    ))}
                    <div className="pb-1" />
                  </div>

                </div>
              </ScrollArea>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>

      <motion.div 
        key="app"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        className="flex h-full"
      >
        <aside className="fixed md:relative z-20 flex w-[300px] h-full flex-col border-r bg-background">
              {/* Sidebar header */}
              <header className="flex h-14 items-center justify-between px-4 border-b">
                <p className="text-sm font-semibold tracking-tight">Meridian Mail</p>
                <button
                  onClick={() => setShowCompose(true)}
                  title="Написать письмо"
                  className="flex h-7 w-7 items-center justify-center rounded-full bg-muted text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Plus className="h-4 w-4" />
                </button>
              </header>

              {/* Search */}
              <div className="px-3 py-2 border-b">
                <Input
                  placeholder="Поиск"
                  className="h-8 bg-muted/40 border-none text-sm"
                  disabled={conversationsState === "loading"}
                />
              </div>

              {/* Conversation list */}
              <ScrollArea className="flex-1">
                <div className="py-1">
                  {conversations.map((conversation) => {
                    const isActive = conversation.id === selectedConversationId;
                    const firstMember = conversation.members[0]?.contact ?? null;
                    const avatarUrl = firstMember?.avatarUrl ?? undefined;
                    const initials =
                      conversation.avatarLetters ||
                      firstMember?.displayName
                        ?.split(" ")
                        .slice(0, 2)
                        .map((word) => word[0]?.toUpperCase())
                        .join("") ||
                      firstMember?.email.slice(0, 2).toUpperCase() ||
                      "MM";
                    return (
                      <button
                        key={conversation.id}
                        onClick={() => handleSelectConversation(conversation.id)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          setConvContextMenu({ x: e.clientX, y: e.clientY, conversationId: conversation.id });
                        }}
                        className={cn(
                          "w-full px-3 py-2.5 text-left transition-colors",
                          "hover:bg-muted/50",
                          isActive && "bg-muted/60",
                        )}
                      >
                        <div className="flex items-center gap-3">
                          <div className="relative shrink-0">
                            <Avatar className="h-10 w-10">
                              {avatarUrl ? <AvatarImage src={avatarUrl} alt={conversation.displayName} /> : null}
                              <AvatarFallback className="bg-muted text-foreground text-xs font-semibold">{initials}</AvatarFallback>
                            </Avatar>
                            {conversation.unreadCount > 0 && !isActive && (
                              <span className="absolute -top-0.5 -right-0.5 h-1.5 w-1.5 rounded-full bg-primary" />
                            )}
                          </div>
                          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                            <div className="flex items-center justify-between gap-2">
                              <p className={cn("truncate text-sm", conversation.unreadCount > 0 && !isActive ? "font-semibold" : "font-medium")}>
                                {conversation.displayName}
                              </p>
                              <span className="shrink-0 text-[10px] text-muted-foreground">
                                {formatTime(conversation.lastMessageAt)}
                              </span>
                            </div>
                            <p className="truncate text-xs text-muted-foreground">
                              {conversation.lastMessagePreview ?? "Нет сообщений"}
                            </p>
                          </div>
                        </div>
                      </button>
                    );
                  })}
                  {conversationsState === "loading" && (
                    <div className="flex items-center justify-center p-8">
                      <div className="h-4 w-4 animate-spin rounded-full border-2 border-foreground/20 border-t-foreground/60" />
                    </div>
                  )}
                  {conversations.length === 0 && conversationsState === "idle" && (
                    <div className="flex flex-col items-center justify-center p-8 text-center gap-2">
                      <Mail className="h-8 w-8 text-muted-foreground/40" />
                      <p className="text-sm text-muted-foreground">Нет переписок</p>
                    </div>
                  )}
                </div>
              </ScrollArea>

              {/* Account footer */}
              <footer className="border-t p-3">
                <button
                  onClick={() => setShowSetup(true)}
                  className="flex items-center gap-3 w-full px-2 py-1.5 rounded-lg hover:bg-muted/60 transition-colors text-left"
                >
                  <div className="h-8 w-8 rounded-full bg-foreground flex items-center justify-center text-background text-[11px] font-bold shrink-0">
                    {selectedAccount ? selectedAccount.email.slice(0, 1).toUpperCase() : <User className="h-4 w-4" />}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-xs font-medium truncate">
                      {selectedAccount ? selectedAccount.displayName || selectedAccount.email.split('@')[0] : "Аккаунт"}
                    </p>
                    <p className="text-[10px] text-muted-foreground truncate">
                      {selectedAccount ? selectedAccount.email : "Подключить ящик"}
                    </p>
                  </div>
                </button>
              </footer>
            </aside>

            <main className="flex flex-1 flex-col bg-background relative overflow-hidden">
              {selectedConversation && conversationHeader ? (
                <>
                  {/* Chat header */}
                  <header
                    className="flex h-14 items-center justify-between px-5 border-b cursor-pointer hover:bg-muted/30 transition-colors"
                    onClick={() => setShowDialogInfo(true)}
                  >
                    <div>
                      <h1 className="text-sm font-semibold">
                        {conversationHeader.displayName}
                      </h1>
                      <p className="text-[11px] text-muted-foreground">
                        {conversationHeader.participantsCount > 2
                          ? `${conversationHeader.participantsCount} участников`
                          : "Диалог"}
                        {" · "}
                        {formatTimestamp(conversationHeader.lastMessageAt, "—")}
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 rounded-full text-muted-foreground hover:text-foreground hover:bg-muted/60"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (selectedAccountId) void performSync(selectedAccountId);
                          if (selectedConversationId) void loadMessages(selectedConversationId);
                        }}
                      >
                        <RefreshCw className={cn("h-4 w-4", (messagesState === "loading" || isSyncing) && "animate-spin")} />
                      </Button>
                      {syncError && (
                        <div className="absolute top-full right-0 mt-2 p-2 bg-destructive/10 text-destructive text-[10px] rounded border border-destructive/20 whitespace-nowrap z-50">
                          {syncError}
                        </div>
                      )}
                    </div>
                  </header>

                  {/* Messages */}
                  <ScrollArea className="flex-1 px-5">
                    <div className="flex flex-col gap-1 py-5 max-w-3xl mx-auto w-full">
                      <AnimatePresence initial={false}>
                        {messages.map((message, index) => {
                          const isNextSameSender =
                            messages[index + 1]?.isOutgoing === message.isOutgoing &&
                            messages[index + 1]?.fromEmail === message.fromEmail;
                          return (
                            <MessageItem
                              key={message.id || index}
                              message={message}
                              isNextSameSender={isNextSameSender}
                              onDelete={handleDeleteMessage}
                            />
                          );
                        })}
                      </AnimatePresence>
                      {messagesState === "loading" && (
                        <div className="flex justify-center p-4">
                          <div className="h-4 w-4 animate-spin rounded-full border-2 border-foreground/20 border-t-foreground/60" />
                        </div>
                      )}
                    </div>
                  </ScrollArea>

                  {/* Compose */}
                  <div className="border-t px-5 py-3">
                    <div className="max-w-3xl mx-auto w-full">
                      <div className="flex items-center gap-3">
                        <div className="flex-1 relative">
                          <Input
                            placeholder="Напишите ответ..."
                            className="pr-12 h-10 rounded-lg bg-muted/40 border-none text-sm focus-visible:ring-1 focus-visible:ring-foreground/20"
                            value={draft}
                            disabled={!selectedAccountId || messagesState === "loading"}
                            onChange={(event) => { setDraft(event.target.value); setSendError(null); }}
                            onKeyDown={(event) => {
                              if (event.key === "Enter" && !event.shiftKey) {
                                event.preventDefault();
                                void handleSend();
                              }
                            }}
                          />
                          <Button
                            size="icon"
                            variant="ghost"
                            className="absolute right-1 top-1 h-8 w-8 rounded-md text-muted-foreground hover:text-foreground disabled:opacity-30"
                            onClick={() => void handleSend()}
                            disabled={!draft.trim() || !selectedAccountId}
                          >
                            <SendHorizontal className="h-4 w-4" />
                          </Button>
                        </div>
                      </div>
                      {sendError ? (
                        <p className="text-[11px] text-destructive mt-1.5 text-center truncate" title={sendError}>{sendError}</p>
                      ) : (
                        <p className="text-[10px] text-muted-foreground/50 mt-1.5 text-center">IMAP/SMTP · без сквозного шифрования</p>
                      )}
                    </div>
                  </div>


                </>
              ) : (
                <div className="flex flex-1 flex-col items-center justify-center text-center">
                  <div className="space-y-2">
                    <p className="text-sm font-medium text-muted-foreground">Выберите диалог</p>
                    <p className="text-xs text-muted-foreground/60">или подключите новый ящик</p>
                  </div>
                </div>
              )}
            </main>
          </motion.div>
      
      {initialLoading && (
        <div className="fixed inset-0 bg-background z-50 flex items-center justify-center">
          <div className="h-5 w-5 animate-spin rounded-full border-2 border-foreground/20 border-t-foreground/70" />
        </div>
      )}

      {/* Conversation context menu */}
      <AnimatePresence>
        {/* Тосты фонового удаления */}
        <div className="fixed bottom-4 left-4 flex flex-col gap-2 z-50 pointer-events-none">
          <AnimatePresence>
            {deleteToasts.map((toast) => (
              <motion.div
                key={toast.id}
                initial={{ opacity: 0, y: 8, scale: 0.96 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: 4, scale: 0.96 }}
                transition={{ duration: 0.15 }}
                className={cn(
                  "flex items-center gap-2.5 px-3.5 py-2.5 rounded-xl text-sm shadow-lg border",
                  toast.status === "error"
                    ? "bg-destructive/10 border-destructive/20 text-destructive"
                    : "bg-background border-border text-foreground"
                )}
              >
                {toast.status === "deleting" && (
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground shrink-0" />
                )}
                {toast.status === "done" && (
                  <Check className="h-3.5 w-3.5 text-green-500 shrink-0" />
                )}
                {toast.status === "error" && (
                  <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                )}
                <span className="text-muted-foreground">
                  {toast.status === "deleting" && "Удаление диалога…"}
                  {toast.status === "done" && "Удалено"}
                  {toast.status === "error" && "Ошибка — диалог восстановлен"}
                </span>
              </motion.div>
            ))}
          </AnimatePresence>
        </div>

        {convContextMenu && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.1 }}
            className="fixed z-50 min-w-[180px] rounded-xl border bg-popover shadow-xl p-1 text-sm"
            style={{ top: convContextMenu.y, left: convContextMenu.x }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-destructive hover:bg-destructive/10 transition-colors"
              onClick={() => {
                void handleDeleteConversation(convContextMenu.conversationId);
                setConvContextMenu(null);
              }}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Удалить диалог
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
