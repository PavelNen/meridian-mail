import React, { useState, useEffect } from "react";
import { createPortal } from "react-dom";
import { motion, AnimatePresence } from "framer-motion";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { cn } from "@/lib/utils";
import { Message } from "@/types";
import { format } from "date-fns";
import { Trash2, Code2, FileText } from "lucide-react";

interface MessageItemProps {
  message: Message;
  isNextSameSender: boolean;
  onDelete?: (messageId: number) => void;
}

const formatTimestamp = (value?: string | Date | null) => {
  if (!value) return "";
  const date = typeof value === "string" ? new Date(value) : value;
  if (Number.isNaN(date.getTime())) return "";
  return format(date, "HH:mm");
};

import DOMPurify from 'dompurify';

export const MessageItem: React.FC<MessageItemProps> = ({
  message,
  isNextSameSender,
  onDelete,
}) => {
  const isOutbound = message.isOutgoing;
  const initials = isOutbound ? "Я" : message.fromEmail?.slice(0, 2).toUpperCase() ?? "?";
  const hasHtml = Boolean(message.bodyHtml);

  // Show HTML by default if available, allow toggle to plain text
  const [showHtml, setShowHtml] = useState(hasHtml);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);

  // Close context menu on outside click
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [contextMenu]);

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  };

  return (
    <>
      <motion.div
        initial={{ opacity: 0, y: 10, scale: 0.97 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        transition={{ duration: 0.18, ease: "easeOut" }}
        className={cn(
          "group flex items-end gap-2 mb-1",
          isOutbound ? "flex-row-reverse" : "flex-row",
        )}
      >
        {!isNextSameSender && (
          <Avatar className="h-6 w-6 shrink-0 mb-1 opacity-80">
            <AvatarFallback className="text-[10px]">{initials}</AvatarFallback>
          </Avatar>
        )}
        {isNextSameSender && <div className="w-6 shrink-0" />}

        <div
          onContextMenu={handleContextMenu}
          className={cn(
            "relative rounded-2xl px-4 py-2.5 text-sm shadow-sm transition-all",
            "max-w-[78%]",
            isOutbound
              ? "bg-primary text-primary-foreground rounded-br-none"
              : "bg-muted text-foreground rounded-bl-none",
            !isNextSameSender && "mb-2",
          )}
        >
          {/* Body */}
          {showHtml && message.bodyHtml ? (
            <div
              className={cn(
                "selectable email-html-content -mx-1 [&_a]:underline [&_a]:text-current [&_img]:max-w-full [&_img]:rounded-md overflow-hidden",
                isOutbound ? "[&_a]:text-primary-foreground/90" : "[&_a]:text-blue-600 dark:[&_a]:text-blue-400"
              )}
              dangerouslySetInnerHTML={{
                __html: DOMPurify.sanitize(message.bodyHtml, {
                  FORBID_TAGS: ['style', 'head', 'meta', 'title', 'script', 'iframe'],
                }),
              }}
            />
          ) : (
            <p className="selectable whitespace-pre-wrap leading-relaxed">
              {message.bodyText || message.bodyHtml || "—"}
            </p>
          )}

          {/* Footer row: toggle + timestamp */}
          <div
            className={cn(
              "mt-1.5 flex items-center justify-between gap-2 text-[10px]",
              "opacity-0 transition-opacity group-hover:opacity-70",
              isOutbound ? "text-primary-foreground" : "text-muted-foreground",
            )}
          >
            {hasHtml && (
              <button
                onClick={() => setShowHtml((v) => !v)}
                className="flex items-center gap-0.5 hover:opacity-100 transition-opacity"
                title={showHtml ? "Показать текст" : "Показать HTML"}
              >
                {showHtml ? (
                  <FileText className="h-3 w-3" />
                ) : (
                  <Code2 className="h-3 w-3" />
                )}
              </button>
            )}
            <span className="ml-auto">{formatTimestamp(message.sentAt)}</span>
          </div>
        </div>
      </motion.div>

      {/* Context menu */}
      {createPortal(
        <AnimatePresence>
          {contextMenu && (
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              transition={{ duration: 0.1 }}
              className="fixed z-[100] min-w-[160px] rounded-xl border bg-popover shadow-xl p-1 text-sm text-foreground"
              style={{ top: contextMenu.y, left: contextMenu.x }}
              onClick={(e) => e.stopPropagation()}
            >
              {onDelete && (
                <button
                  className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-destructive hover:bg-destructive/10 transition-colors"
                  onClick={() => {
                    onDelete(message.id);
                    setContextMenu(null);
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  Удалить сообщение
                </button>
              )}
            </motion.div>
          )}
        </AnimatePresence>,
        document.getElementById('root') || document.body
      )}
    </>
  );
};
