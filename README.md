# Meridian Mail

A messenger built on open email protocols — IMAP and SMTP. No proprietary servers, no lock-in. Your data lives on your own email provider.

> **Status:** Early prototype / proof of concept. Not production-ready.

## Idea

Standard messengers (Telegram, WhatsApp, Signal) can be blocked at the country level, deplatformed, or shut down. Email infrastructure cannot — it's federated, open, and has existed for 50 years.

Meridian Mail takes that infrastructure and wraps it in a messenger-style UX: conversation threads, instant compose, no visible subject lines or headers.

## Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | [Tauri 2](https://tauri.app) |
| Frontend | React 19 + TypeScript + Vite |
| Backend | Rust (async, Tokio) |
| Email (receiving) | IMAP with IDLE push via `async-imap` |
| Email (sending) | SMTP via `lettre` |
| Local storage | SQLite via `rusqlite` |
| Credentials | System keychain — macOS Keychain / Windows Credential Manager / Linux Secret Service (`keyring`) |

## Getting started

### Prerequisites

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 20+
- [pnpm](https://pnpm.io) — `npm i -g pnpm`
- Tauri CLI — `cargo install tauri-cli`
- **Linux only:** `libsecret-1-dev` and `pkg-config` for the system keychain

### Run in development

```bash
pnpm install
pnpm tauri dev
```

### Build for release

```bash
pnpm tauri build
```

Output bundles are in `src-tauri/target/release/bundle/`.

## Connecting an account

Meridian Mail uses **app passwords**, not your main account password.

| Provider | How to get an app password |
|----------|---------------------------|
| Gmail | [myaccount.google.com/apppasswords](https://myaccount.google.com/apppasswords) |
| Yandex | [id.yandex.ru/security/app-passwords](https://id.yandex.ru/security/app-passwords) |
| Other | Any IMAP/SMTP provider with standard ports (993/465) |

## Architecture

```
React (Vite)  ──IPC──  Rust (Tauri)
                            │
                   ┌────────┴────────┐
                   │                 │
               IMAP IDLE          SMTP send
               (push sync)       (outgoing)
                   │
               SQLite (local cache)
```

Conversation threading follows RFC 5322: `In-Reply-To` and `References` headers are used to group messages. New conversations are matched by participant set.

## Limitations

- No end-to-end encryption (IMAP/SMTP do not provide it natively)
- Attachments are detected but not yet downloadable
- Search UI is present but not implemented
- Linux requires a running Secret Service daemon (e.g. GNOME Keyring or KWallet)

## License

MIT — see [LICENSE](LICENSE).
