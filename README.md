# Meridian Mail

Мессенджер на базе открытых протоколов электронной почты — IMAP и SMTP. Никаких проприетарных серверов, никакой зависимости от платформы. Ваши данные хранятся у вашего провайдера электронной почты.

> **Статус:** ранний прототип, не готов к продакшену.

## Идея

Обычные мессенджеры (Telegram, WhatsApp, Signal) могут быть заблокированы на уровне страны, отключены или закрыты. Email-инфраструктура — нет: она федеративная, открытая и существует уже 50 лет.

Meridian Mail берёт эту инфраструктуру и оборачивает её в UX мессенджера: цепочки переписок, мгновенный набор сообщений, никаких тем и заголовков на виду.

## Стек

| Слой | Технология |
|------|-----------|
| Десктоп-оболочка | [Tauri 2](https://tauri.app) |
| Фронтенд | React 19 + TypeScript + Vite |
| Бэкенд | Rust (async, Tokio) |
| Получение почты | IMAP с IDLE push через `async-imap` |
| Отправка почты | SMTP через `lettre` |
| Локальное хранилище | SQLite через `rusqlite` |
| Пароли | Системное хранилище ОС — macOS Keychain / Windows Credential Manager / Linux Secret Service (`keyring`) |

## Быстрый старт

### Зависимости

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 20+
- [pnpm](https://pnpm.io) — `npm i -g pnpm`
- Tauri CLI — `cargo install tauri-cli`
- **Только Linux:** `libsecret-1-dev` и `pkg-config` для системного хранилища паролей

### Запуск в режиме разработки

```bash
pnpm install
pnpm tauri dev
```

### Сборка релиза

```bash
pnpm tauri build
```

Готовые бандлы появятся в `src-tauri/target/release/bundle/`.

## Подключение аккаунта

Meridian Mail использует **пароли приложений**, а не основной пароль от почты.

| Провайдер | Как получить пароль приложения |
|-----------|-------------------------------|
| Gmail | [myaccount.google.com/apppasswords](https://myaccount.google.com/apppasswords) |
| Яндекс | [id.yandex.ru/security/app-passwords](https://id.yandex.ru/security/app-passwords) |
| Другие | Любой IMAP/SMTP провайдер со стандартными портами (993/465) |

## Архитектура

```
React (Vite)  ──IPC──  Rust (Tauri)
                             │
                    ┌────────┴────────┐
                    │                 │
                IMAP IDLE          SMTP send
                (push-sync)       (отправка)
                    │
                SQLite (локальный кэш)
```

Группировка переписок следует RFC 5322: заголовки `In-Reply-To` и `References` используются для объединения сообщений в тред. Новые диалоги сопоставляются по набору участников.

## Ограничения

- Нет сквозного шифрования (IMAP/SMTP не предоставляют его на уровне протокола)
- Вложения определяются, но пока недоступны для скачивания
- Поиск в UI есть, но не реализован
- Linux требует запущенного демона Secret Service (например, GNOME Keyring или KWallet)

## Участие в разработке

Проект открыт для контрибуций. Баги, идеи, PR — всё приветствуется.
См. [CONTRIBUTING.md](CONTRIBUTING.md) для деталей.

## Лицензия

MIT — см. [LICENSE](LICENSE).
