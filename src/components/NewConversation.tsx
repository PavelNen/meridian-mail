import { useCallback, useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, SendHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { cn } from "@/lib/utils";
import { sendMessage, searchContacts } from "../commands";
import type { Contact, Recipient, Message } from "../types";

interface Props {
  accountId: number;
  onSent: (message: Message) => void;
  onClose: () => void;
}

function isValidEmail(value: string) {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value.trim());
}

function recipientInitials(r: Recipient) {
  if (r.name) return r.name.slice(0, 2).toUpperCase();
  return r.email.slice(0, 2).toUpperCase();
}

export function NewConversation({ accountId, onSent, onClose }: Props) {
  const [recipients, setRecipients] = useState<Recipient[]>([]);
  const [toInput, setToInput] = useState("");
  const [suggestions, setSuggestions] = useState<Contact[]>([]);
  const [activeSuggestion, setActiveSuggestion] = useState(0);
  const [body, setBody] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Search contacts as user types
  useEffect(() => {
    if (searchTimer.current) clearTimeout(searchTimer.current);
    if (toInput.trim().length < 1) {
      setSuggestions([]);
      return;
    }
    searchTimer.current = setTimeout(async () => {
      try {
        const results = await searchContacts(toInput.trim());
        // filter out already-added recipients
        setSuggestions(
          results.filter((c) => !recipients.some((r) => r.email === c.email)),
        );
        setActiveSuggestion(0);
      } catch {
        setSuggestions([]);
      }
    }, 120);
    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [toInput, recipients]);

  const addRecipient = useCallback(
    (r: Recipient) => {
      if (recipients.some((x) => x.email === r.email)) return;
      setRecipients((prev) => [...prev, r]);
      setToInput("");
      setSuggestions([]);
      bodyRef.current?.focus();
    },
    [recipients],
  );

  const removeRecipient = useCallback((email: string) => {
    setRecipients((prev) => prev.filter((r) => r.email !== email));
  }, []);

  const commitInput = useCallback(() => {
    const val = toInput.trim();
    if (isValidEmail(val)) {
      addRecipient({ email: val });
    }
  }, [toInput, addRecipient]);

  const handleToKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveSuggestion((i) => Math.min(i + 1, suggestions.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveSuggestion((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" || e.key === "Tab" || e.key === ",") {
      e.preventDefault();
      if (suggestions.length > 0) {
        const s = suggestions[activeSuggestion];
        addRecipient({ email: s.email, name: s.displayName ?? undefined });
      } else {
        commitInput();
      }
    } else if (e.key === "Backspace" && toInput === "" && recipients.length > 0) {
      setRecipients((prev) => prev.slice(0, -1));
    } else if (e.key === "Escape") {
      if (suggestions.length > 0) {
        setSuggestions([]);
      } else {
        onClose();
      }
    }
  };

  const handleSend = async () => {
    // Commit any pending input first
    const pending = toInput.trim();
    let finalRecipients = recipients;
    if (isValidEmail(pending)) {
      finalRecipients = [...recipients, { email: pending }];
      setRecipients(finalRecipients);
      setToInput("");
    }

    if (finalRecipients.length === 0) {
      setError("Укажите хотя бы одного получателя");
      toRef.current?.focus();
      return;
    }
    if (!body.trim()) {
      setError("Напишите сообщение");
      bodyRef.current?.focus();
      return;
    }

    setSending(true);
    setError(null);
    try {
      // Subject is auto-generated from the first line of the body — hidden from user
      const subject = body.trim().split("\n")[0].slice(0, 80) || "—";
      const message = await sendMessage(accountId, {
        to: finalRecipients,
        body: body.trim(),
        subject,
      });
      onSent(message);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSending(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b">
        <span className="text-sm font-semibold">Новое сообщение</span>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 rounded-full text-muted-foreground"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* To field */}
      <div className="relative border-b px-4 py-2">
        <div className="flex flex-wrap items-center gap-1.5 min-h-[32px]">
          <span className="text-[11px] text-muted-foreground font-medium shrink-0">Кому</span>
          {recipients.map((r) => (
            <span
              key={r.email}
              className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium"
            >
              <Avatar className="h-4 w-4">
                <AvatarFallback className="text-[8px] bg-primary/10 text-primary">
                  {recipientInitials(r)}
                </AvatarFallback>
              </Avatar>
              {r.name || r.email.split("@")[0]}
              <button
                className="text-muted-foreground hover:text-foreground ml-0.5 leading-none"
                onClick={() => removeRecipient(r.email)}
                tabIndex={-1}
              >
                <X className="h-2.5 w-2.5" />
              </button>
            </span>
          ))}
          <input
            ref={toRef}
            value={toInput}
            onChange={(e) => setToInput(e.target.value)}
            onKeyDown={handleToKeyDown}
            onBlur={() => {
              // small delay so click on suggestion registers
              setTimeout(() => {
                commitInput();
                setSuggestions([]);
              }, 150);
            }}
            placeholder={recipients.length === 0 ? "Email или имя..." : ""}
            className="flex-1 min-w-[120px] bg-transparent text-sm outline-none placeholder:text-muted-foreground/50"
            autoFocus
          />
        </div>

        {/* Autocomplete dropdown */}
        <AnimatePresence>
          {suggestions.length > 0 && (
            <motion.div
              initial={{ opacity: 0, y: -4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.1 }}
              className="absolute left-0 right-0 top-full z-50 border-b bg-background shadow-lg"
            >
              {suggestions.slice(0, 6).map((contact, i) => (
                <button
                  key={contact.email}
                  className={cn(
                    "flex w-full items-center gap-3 px-4 py-2 text-left text-sm transition-colors hover:bg-muted/60",
                    i === activeSuggestion && "bg-muted/60",
                  )}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    addRecipient({
                      email: contact.email,
                      name: contact.displayName ?? undefined,
                    });
                  }}
                >
                  <Avatar className="h-7 w-7 shrink-0">
                    <AvatarFallback className="text-[10px] bg-primary/10 text-primary font-bold">
                      {(contact.displayName || contact.email).slice(0, 2).toUpperCase()}
                    </AvatarFallback>
                  </Avatar>
                  <div className="min-w-0">
                    {contact.displayName && (
                      <p className="truncate font-medium text-xs">{contact.displayName}</p>
                    )}
                    <p className="truncate text-[11px] text-muted-foreground">{contact.email}</p>
                  </div>
                </button>
              ))}
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* Body */}
      <div className="flex-1 px-4 py-3">
        <textarea
          ref={bodyRef}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Сообщение..."
          className="w-full h-full resize-none bg-transparent text-sm outline-none placeholder:text-muted-foreground/50"
        />
      </div>

      {/* Footer */}
      <div className="border-t px-4 py-3 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          {error && (
            <p className="text-[11px] text-destructive">{error}</p>
          )}
          {!error && (
            <p className="text-[10px] text-muted-foreground/40">⌘↵ отправить</p>
          )}
        </div>
        <Button
          size="sm"
          className="h-8 px-4 rounded-lg gap-1.5"
          disabled={sending || (recipients.length === 0 && !isValidEmail(toInput)) || !body.trim()}
          onClick={() => void handleSend()}
        >
          {sending ? (
            <div className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-background/40 border-t-background" />
          ) : (
            <SendHorizontal className="h-3.5 w-3.5" />
          )}
          Отправить
        </Button>
      </div>
    </div>
  );
}
