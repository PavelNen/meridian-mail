# Contributing to Meridian Mail

Спасибо за интерес к проекту. Это ранний прототип — любая помощь ценна.

## Как запустить локально

**Зависимости:**
- [Rust](https://rustup.rs) stable
- Node.js 20+, [pnpm](https://pnpm.io)
- **Linux:** `sudo apt install libwebkit2gtk-4.1-dev libsecret-1-dev pkg-config`

**Запуск:**
```bash
git clone https://github.com/PavelNen/meridian-mail.git
cd meridian-mail
pnpm install
pnpm tauri dev
```

## Как помочь

### Нашли баг
Откройте [issue](https://github.com/PavelNen/meridian-mail/issues) по шаблону — опишите что произошло, что ожидали, платформу и версию.

### Хотите добавить фичу
Сначала откройте issue и обсудите идею — чтобы не делать работу впустую.

### Хотите написать код
1. Форкните репозиторий
2. Создайте ветку: `git checkout -b fix/something` или `feat/something`
3. Сделайте изменения, проверьте что `pnpm tauri dev` запускается
4. Откройте Pull Request с описанием что и зачем изменили

## Что сейчас особенно нужно

- [ ] Поиск по сообщениям (UI есть, backend нет)
- [ ] Скачивание вложений
- [ ] Reply с правильным `In-Reply-To` заголовком
- [ ] Quote stripping для разных форматов (Outlook, Apple Mail)
- [ ] Тестирование с self-hosted серверами (Postfix, Stalwart, Maddy)
- [ ] Тестирование на Windows и Linux

## Структура проекта

```
src/                  # React фронтенд (TypeScript)
  components/         # UI-компоненты
  App.tsx             # Главный компонент, состояние
src-tauri/
  src/
    commands/         # Tauri IPC-команды (вызываются из фронтенда)
    db/               # SQLite: схема и запросы
    imap/             # IMAP клиент + IDLE
    smtp/             # Отправка писем
    keychain.rs       # Хранение паролей через системный keychain
    models/           # Общие структуры данных
```

## Код

- Rust: `cargo clippy` не должен давать ошибок
- TypeScript: `pnpm build` (tsc) не должен давать ошибок
- Стиль — следуйте тому, что уже есть в коде

## Вопросы

Открывайте issue с тегом `question` или пишите в Discussions.
