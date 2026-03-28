import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Lock,
  AlertCircle,
  Trash2,
  Settings2,
  LayoutDashboard,
  X,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { ScrollArea } from "./ui/scroll-area";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listAccounts, removeAccount, addAccount } from "../commands";
import type { Account, NewAccountInput } from "../types";

interface AccountSetupProps {
  onComplete: () => void;
  onAccountsChanged?: () => void;
  onCancel?: () => void;
  selectedAccountId?: number | null;
  onSelectAccount?: (id: number) => void;
}

type Provider = "google" | "yandex";
type SetupStep = "choice" | "manual" | "manage" | "provider";

export function AccountSetup({
  onComplete,
  onAccountsChanged,
  onCancel,
  selectedAccountId,
  onSelectAccount,
}: AccountSetupProps) {
  const [step, setStep] = useState<SetupStep>("choice");
  const [provider, setProvider] = useState<Provider | null>(null);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [form, setForm] = useState<NewAccountInput>({
    email: "",
    displayName: "",
    password: "",
    imapHost: "",
    imapPort: 993,
    smtpHost: "",
    smtpPort: 465,
  });

  useEffect(() => {
    if (step === "manage") void loadExistingAccounts();
  }, [step]);

  const loadExistingAccounts = async () => {
    setLoading(true);
    try {
      const list = await listAccounts();
      setAccounts(list);
    } catch {
      setError("Не удалось загрузить аккаунты");
    } finally {
      setLoading(false);
    }
  };

  const handleRemove = async (id: number) => {
    setLoading(true);
    try {
      await removeAccount(id);
      await loadExistingAccounts();
      onAccountsChanged?.();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      await addAccount(form);
      onComplete();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  const handleProviderSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      if (provider === "google") {
        await addAccount({
          ...form,
          imapHost: "imap.gmail.com",
          imapPort: 993,
          smtpHost: "smtp.gmail.com",
          smtpPort: 465,
        });
      } else if (provider === "yandex") {
        await addAccount({
          ...form,
          imapHost: "imap.yandex.ru",
          imapPort: 993,
          smtpHost: "smtp.yandex.ru",
          smtpPort: 465,
        });
      }
      onComplete();
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setLoading(false);
    }
  };

  // Derive header title and back action from current step
  const headerTitle =
    step === "choice" ? "Подключить ящик" :
    step === "provider" ? (provider === "google" ? "Google (Gmail)" : "Яндекс Почта") :
    step === "manual" ? "Ручная настройка" :
    "Управление ящиками";

  const handleBack = step !== "choice" ? () => setStep("choice") : undefined;

  return (
    <div className="flex flex-col">
      {/* Modal header */}
      <div className="flex h-12 items-center gap-2 px-4 border-b shrink-0">
        {handleBack ? (
          <Button
            variant="ghost"
            size="icon"
            type="button"
            onClick={handleBack}
            className="h-7 w-7 rounded-full text-muted-foreground hover:text-foreground shrink-0 -ml-1"
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
        ) : null}
        <p className="text-sm font-semibold flex-1 truncate">{headerTitle}</p>
        {onCancel && (
          <Button
            variant="ghost"
            size="icon"
            onClick={onCancel}
            className="h-7 w-7 rounded-full text-muted-foreground hover:text-foreground shrink-0"
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>

      <div className="flex flex-col gap-4 p-4">

      <AnimatePresence mode="wait">
        {/* ── CHOICE ── */}
        {step === "choice" && (
          <motion.div
            key="choice"
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            className="grid gap-2"
          >
            {/* Yandex */}
            <button
              onClick={() => {
                setProvider("yandex");
                setForm({ ...form, password: "", imapHost: "", smtpHost: "" });
                setStep("provider");
              }}
              className="flex items-center gap-4 rounded-lg bg-muted/40 px-4 py-3.5 text-left transition-colors hover:bg-muted/70"
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-md bg-foreground text-background text-sm font-bold shrink-0">
                Я
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium">Яндекс Почта</p>
                <p className="text-[11px] text-muted-foreground">Пароль приложения</p>
              </div>
              <ChevronRight className="h-4 w-4 text-muted-foreground/40 shrink-0" />
            </button>

            {/* Google */}
            <button
              onClick={() => {
                setProvider("google");
                setForm({ ...form, password: "", imapHost: "", smtpHost: "" });
                setStep("provider");
              }}
              className="flex items-center gap-4 rounded-lg bg-muted/40 px-4 py-3.5 text-left transition-colors hover:bg-muted/70"
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-md bg-foreground text-background text-sm font-bold shrink-0">
                G
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium">Google (Gmail)</p>
                <p className="text-[11px] text-muted-foreground">App Password</p>
              </div>
              <ChevronRight className="h-4 w-4 text-muted-foreground/40 shrink-0" />
            </button>

            <div className="border-t my-1" />

            {/* Manual */}
            <button
              onClick={() => setStep("manual")}
              className="flex items-center gap-4 rounded-lg px-4 py-3 text-left transition-colors hover:bg-muted/40"
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-md bg-muted text-muted-foreground shrink-0">
                <Settings2 className="h-4 w-4" />
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium">Ручная настройка</p>
                <p className="text-[11px] text-muted-foreground">Корпоративная почта и другие</p>
              </div>
              <ChevronRight className="h-4 w-4 text-muted-foreground/40 shrink-0" />
            </button>

            <button
              onClick={() => setStep("manage")}
              className="flex items-center gap-4 rounded-lg px-4 py-3 text-left transition-colors hover:bg-muted/40"
            >
              <div className="flex h-9 w-9 items-center justify-center rounded-md bg-muted text-muted-foreground shrink-0">
                <LayoutDashboard className="h-4 w-4" />
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium">Управление аккаунтами</p>
                <p className="text-[11px] text-muted-foreground">Просмотр и удаление ящиков</p>
              </div>
              <ChevronRight className="h-4 w-4 text-muted-foreground/40 shrink-0" />
            </button>
          </motion.div>
        )}

        {/* ── PROVIDER ── */}
        {step === "provider" && provider && (
          <motion.form
            key="provider"
            initial={{ opacity: 0, x: 16 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -16 }}
            onSubmit={handleProviderSubmit}
            className="space-y-4"
          >

            <div className="grid gap-3">
              <div className="flex gap-3">
                <div className="space-y-1 flex-1">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    Email
                  </label>
                  <Input
                    placeholder={provider === "google" ? "name@gmail.com" : "name@yandex.ru"}
                    value={form.email}
                    onChange={(e) => setForm({ ...form, email: e.target.value })}
                    required
                  />
                </div>
                <div className="space-y-1 flex-[0.7]">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    Имя
                  </label>
                  <Input
                    placeholder="Павел Н."
                    value={form.displayName || ""}
                    onChange={(e) => setForm({ ...form, displayName: e.target.value })}
                  />
                </div>
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                  Пароль приложения
                </label>
                <div className="relative">
                  <Input
                    type="password"
                    placeholder="••••••••••••••••"
                    value={form.password}
                    onChange={(e) => setForm({ ...form, password: e.target.value })}
                    required
                  />
                  <Lock className="absolute right-3 top-2.5 h-4 w-4 text-muted-foreground/50" />
                </div>
                <p className="text-[11px] text-muted-foreground pt-1 leading-relaxed">
                  Используется пароль приложения, не основной пароль от ящика.{" "}
                  <button
                    type="button"
                    onClick={() =>
                      void openUrl(
                        provider === "google"
                          ? "https://myaccount.google.com/apppasswords"
                          : "https://id.yandex.ru/security/app-passwords"
                      )
                    }
                    className="text-primary underline underline-offset-2 hover:opacity-80"
                  >
                    Создать →
                  </button>
                </p>
              </div>
            </div>

            {error && (
              <div className="flex items-center gap-2 rounded-md bg-destructive/10 p-3 text-xs text-destructive">
                <AlertCircle className="h-4 w-4 shrink-0" />
                <p>{error}</p>
              </div>
            )}

            <Button className="w-full h-10" type="submit" disabled={loading}>
              {loading ? "Подключение..." : "Войти"}
            </Button>
          </motion.form>
        )}

        {/* ── MANUAL ── */}
        {step === "manual" && (
          <motion.form
            key="manual"
            initial={{ opacity: 0, x: 16 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -16 }}
            onSubmit={handleSubmit}
            className="space-y-4"
          >

            <div className="grid gap-3">
              <div className="flex gap-3">
                <div className="space-y-1 flex-1">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    Email
                  </label>
                  <Input
                    placeholder="name@example.com"
                    value={form.email}
                    onChange={(e) => setForm({ ...form, email: e.target.value })}
                    required
                  />
                </div>
                <div className="space-y-1 flex-[0.7]">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    Имя
                  </label>
                  <Input
                    placeholder="Павел Н."
                    value={form.displayName || ""}
                    onChange={(e) => setForm({ ...form, displayName: e.target.value })}
                  />
                </div>
              </div>

              <div className="space-y-1">
                <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                  Пароль
                </label>
                <div className="relative">
                  <Input
                    type="password"
                    placeholder="••••••••"
                    value={form.password}
                    onChange={(e) => setForm({ ...form, password: e.target.value })}
                    required
                  />
                  <Lock className="absolute right-3 top-2.5 h-4 w-4 text-muted-foreground/50" />
                </div>
              </div>

              <div className="border-t pt-1" />

              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    IMAP Host
                  </label>
                  <Input
                    placeholder="imap.gmail.com"
                    value={form.imapHost}
                    onChange={(e) => setForm({ ...form, imapHost: e.target.value })}
                    required
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                    SMTP Host
                  </label>
                  <Input
                    placeholder="smtp.gmail.com"
                    value={form.smtpHost}
                    onChange={(e) => setForm({ ...form, smtpHost: e.target.value })}
                    required
                  />
                </div>
              </div>
            </div>

            {error && (
              <div className="flex items-center gap-2 rounded-md bg-destructive/10 p-3 text-xs text-destructive">
                <AlertCircle className="h-4 w-4" />
                <p>{error}</p>
              </div>
            )}

            <Button className="w-full h-10" type="submit" disabled={loading}>
              {loading ? "Подключение..." : "Завершить настройку"}
            </Button>
          </motion.form>
        )}

        {/* ── MANAGE ── */}
        {step === "manage" && (
          <motion.div
            key="manage"
            initial={{ opacity: 0, x: 16 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -16 }}
            className="space-y-4"
          >

            <ScrollArea className="h-[280px]">
              <div className="grid gap-1.5 pr-4">
                {loading && (
                  <p className="text-sm text-center py-8 text-muted-foreground animate-pulse">
                    Загрузка...
                  </p>
                )}
                {!loading && accounts.length === 0 && (
                  <div className="text-center py-8">
                    <p className="text-sm text-muted-foreground">Нет подключенных ящиков.</p>
                    <Button
                      variant="ghost"
                      className="text-primary mt-2 text-sm"
                      onClick={() => setStep("choice")}
                    >
                      Добавить
                    </Button>
                  </div>
                )}
                {accounts.map((acc) => (
                  <div
                    key={acc.id}
                    className="flex items-center justify-between gap-3 rounded-lg bg-muted/40 px-3 py-2.5"
                  >
                    <div className="flex items-center gap-3 overflow-hidden">
                      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-foreground text-background text-[10px] font-bold">
                        {acc.email.slice(0, 1).toUpperCase()}
                      </div>
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">
                          {acc.displayName || acc.email}
                        </p>
                        <p className="truncate text-[10px] text-muted-foreground">{acc.email}</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-1.5">
                      {onSelectAccount && (
                        <Button
                          variant={selectedAccountId === acc.id ? "default" : "ghost"}
                          size="sm"
                          className="h-7 text-xs"
                          onClick={() => {
                            onSelectAccount(acc.id);
                            onCancel?.();
                          }}
                          disabled={selectedAccountId === acc.id}
                        >
                          {selectedAccountId === acc.id ? "Активен" : "Выбрать"}
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => handleRemove(acc.id)}
                        className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive h-7 w-7"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </ScrollArea>
          </motion.div>
        )}
      </AnimatePresence>
      </div>
    </div>
  );
}
