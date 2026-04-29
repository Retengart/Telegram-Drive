# External Integrations

**Analysis Date:** 2026-04-29

## APIs & External Services

**Telegram MTProto (sole external API):**
- Service: Telegram Cloud — both data plane and "drive" backend.
- SDK/Client: `grammers-client` + `grammers-mtsender` + `grammers-tl-types` + `grammers-session`, all pinned to git rev `d07f96f` in `app/src-tauri/Cargo.toml`.
- Auth: per-user OAuth-style flow using **user-supplied `api_id` (i32) and `api_hash` (string)** from `https://my.telegram.org`. Plus phone code, plus optional 2FA password. No app-level shared API credentials — every install needs its own.
- Endpoints called (raw TL functions):
  - `channels.CreateChannel` — `app/src-tauri/src/commands/fs.rs:33` (broadcast channel as "folder").
  - `messages.SetHistoryTtl` — `fs.rs:62` (period=0, disables TTL on new folder).
  - `channels.DeleteChannel` — `fs.rs:103`.
  - `channels.GetFullChannel` — `fs.rs:456` (folder discovery via about-text marker).
  - `messages.SearchGlobal` — `fs.rs:346` with `InputMessagesFilterDocument`, limit 50.
  - High-level wrappers: `request_login_code`, `sign_in`, `check_password`, `sign_out` (auth.rs); `iter_dialogs`, `iter_messages`, `get_messages_by_id`, `iter_download`, `download_media`, `upload_file`, `send_message`, `forward_messages`, `delete_messages`, `get_me` (across fs.rs / preview.rs / server.rs).
- Connectivity probe: raw TCP connect to `149.154.167.50:443` (Telegram DC2) with 2 s timeout — `app/src-tauri/src/commands/network.rs:13`. Avoids grammers reconnection bugs.
- Folder identification: private channels with title suffix ` [TD]` and/or about-text marker `[telegram-drive-folder]` (`fs.rs::cmd_scan_folders`).
- "Saved Messages" pseudo-folder: `folder_id == None` resolves to the user's own peer via `client.get_me()` (`commands/utils.rs::resolve_peer`).
- Error mapping: `FLOOD_WAIT_<n>` extraction in `commands/utils.rs::map_error`. AUTH_RESTART / 500 → 1 retry in `auth.rs::cmd_auth_request_code`.

**No other third-party HTTP APIs.** Frontend never makes outbound network calls except via the Tauri updater plugin (see "CI/CD & Deployment").

## Data Storage

**Databases:**
- SQLite — solely for grammers session persistence via `SqliteSession::open` (`app/src-tauri/src/commands/auth.rs:64`).
  - Path: `<app_data_dir>/telegram.session` (+ `-wal`, `-shm` sidecars).
  - Corruption recovery: on open error, all three files deleted and re-created (`auth.rs:67–74`).
  - Wiped on logout (`auth.rs:190–194`).

**File Storage:**
- Local filesystem only. No S3/GCS/Azure blob.
- App data dir (`tauri::path::app_data_dir`):
  - `telegram.session{,-wal,-shm}` — grammers SQLite session.
  - `bandwidth.json` — daily byte counters (see "Bandwidth gate" below).
  - `thumbnails/` — image thumbnail cache, **no pruning**.
  - `config.json` (current) / `settings.json` (legacy) — `tauri-plugin-store` user config.
- App cache dir (`tauri::path::app_cache_dir`):
  - `previews/` — preview download cache. LRU-pruned to **max 30 files / 80 MB** (`commands/preview.rs:9–10`, `prune_preview_cache`). Wiped on `cmd_clean_cache` (logout).

**Caching:**
- In-memory: `@tanstack/react-query` cache in the React frontend (`app/src/App.tsx`), keyed per-folder.
- Disk: `previews/`, `thumbnails/` (above).
- HTTP: streaming server emits `Cache-Control: private, max-age=120` on responses (`server.rs:81`).

## Authentication & Identity

**Auth Provider:**
- Telegram itself — phone-code (SMS / TG message) + optional 2FA cloud password.
- Implementation: `app/src-tauri/src/commands/auth.rs`.
  - `cmd_auth_request_code` → stores `LoginToken` in `TelegramState.login_token`.
  - `cmd_auth_sign_in` → on `SignInError::PasswordRequired`, stashes `PasswordToken` and routes UI to "password" step.
  - `cmd_auth_check_password` → `client.check_password`.
  - `cmd_logout` → signals runner shutdown, calls `client.sign_out()`, wipes session files, clears `TelegramState`.
- Reconnect: `cmd_check_connection` pings `client.get_me()`; on failure re-runs `ensure_client_initialized` using cached `api_id` from `TelegramState.api_id`.

**Streaming server token:**
- Random 32-char hex generated **once per app launch** (`lib.rs::generate_stream_token`) using `rand::thread_rng`.
- Stored in Tauri-managed state `StreamToken` (`commands/streaming.rs`).
- Frontend fetches via `cmd_get_stream_token` and appends `?token=...` to every `http://localhost:14200/stream/...` URL.
- Server validates exact-match in `server.rs::stream_media:27` — 403 on mismatch/missing.
- Not persisted; rotates each launch.

## Monitoring & Observability

**Error Tracking:**
- None (no Sentry / Bugsnag / Rollbar). User-visible errors surface via `sonner` toasts and an in-app `ErrorBoundary` (`app/src/components/ErrorBoundary.tsx`).

**Logs:**
- Rust: `log` + `env_logger` (initialised in `lib.rs:28`). Level via `RUST_LOG` env var. Stdout/stderr only.
- Frontend → Rust bridge: `cmd_log` (`commands/utils.rs:27`) — frontend-prefixed log line forwarded to `log::info!`.

## CI/CD & Deployment

**Hosting:**
- GitHub Releases — installers attached per tag (`.github/workflows/release.yml`).
- No backend hosting / no servers operated.

**CI Pipeline:**
- GitHub Actions — `.github/workflows/release.yml`.
- Trigger: push of tag matching `v*`.
- Jobs: `create-release` (draft) → `build-tauri` (matrix: windows-latest, ubuntu-22.04, macos-latest x86_64, macos-latest aarch64) → `publish-release` (un-drafts).
- Build action: `tauri-apps/tauri-action@v0`.
- Linux post-build: AppImage extracted, bundled `libEGL/GL/GLdispatch/GLX/GLESv2` libs deleted from squashfs, custom `AppRun` wrapper injected (lines 163–230 of release.yml). Repack uses `appimagetool-x86_64.AppImage` with `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE needed).
- Code signing: Tauri Ed25519 signature on update artifacts via `TAURI_SIGNING_PRIVATE_KEY{,_PASSWORD}` GitHub secrets (`release.yml:103-104`).

**Auto-update:**
- Plugin: `tauri-plugin-updater` 2.9.0 (Rust) + `@tauri-apps/plugin-updater` 2.10.0 (frontend).
- Update manifest endpoint: `https://github.com/caamer20/Telegram-Drive/releases/latest/download/latest.json` (`tauri.conf.json:6`).
- Public verification key (Ed25519, base64-encoded minisign pubkey) pinned in `tauri.conf.json:8`.
- Frontend flow: `app/src/hooks/useUpdateCheck.ts` — checks 5 s after launch, surfaces banner via `app/src/components/UpdateBanner.tsx`, calls `update.downloadAndInstall(...)` with progress, then `relaunch()` from `@tauri-apps/plugin-process`.

## Environment Configuration

**Required secrets (CI only):**
- `TAURI_SIGNING_PRIVATE_KEY` — Ed25519 minisign secret key for updater signatures.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password for that key.
- `GITHUB_TOKEN` — release upload (provided by Actions).

**Required runtime input (per user):**
- Telegram `api_id` (numeric) and `api_hash` — entered in `AuthWizard`, persisted via `tauri-plugin-store` in `<app_data_dir>/config.json`. Not git-committed. **NOT** an env var.

**Secrets location:**
- CI: GitHub repository secrets.
- User: Tauri-managed app data dir (OS-dependent; e.g. Linux `~/.local/share/com.cameronamer.telegramdrive/`).
- No `.env` files are read at runtime.

## Webhooks & Callbacks

**Incoming:**
- Local HTTP server only (Actix on `127.0.0.1:14200`):
  - `GET /stream/{folder_id}/{message_id}?token={hex}` — chunked-streaming proxy from Telegram media to the WebView. `folder_id` may be the literal `me`, `home`, or `null` for Saved Messages, otherwise `i64` channel id. Returns 403 on token mismatch, 400 on bad folder id, 503 on no-client, 404 on missing message/media. (`app/src-tauri/src/server.rs:19–95`.)
- CORS allowed origins (`server.rs:111`): `tauri://localhost`, `http://localhost:1420`, `https://tauri.localhost`.

**Outgoing:**
- All Telegram MTProto traffic via grammers (no HTTP webhooks).
- Single HTTPS GET on `https://github.com/.../latest.json` for updater manifest.

## OS Surfaces (Tauri capabilities)

Granted in `app/src-tauri/capabilities/default.json` for the `main` window:
- `core:default` — Tauri core IPC.
- `shell:allow-open` — `@tauri-apps/plugin-shell` `open()` for external URLs (e.g. `my.telegram.org` link in AuthWizard).
- `store:default` — full `tauri-plugin-store` access (`config.json`, `settings.json`).
- `dialog:default` — native open/save dialogs (file picker for upload, folder picker for download — `app/src/hooks/useFileUpload.ts`, `useFileDownload.ts`).
- `fs:default` plus `fs:allow-appdata-read-recursive`, `fs:allow-appdata-write-recursive`, `fs:allow-appdata-meta-recursive` — read/write/stat under app-data only.
- `updater:default` — updater plugin permissions.

**Content Security Policy** (`tauri.conf.json:32`):
- `default-src 'self'`
- `connect-src 'self' http://localhost:14200`
- `media-src 'self' http://localhost:14200` — required for `<video>`/`<audio>` consuming the streaming server.
- `img-src 'self' data: blob: asset: https://asset.localhost`
- `style-src 'self' 'unsafe-inline'`
- `script-src 'self'`
- `worker-src 'self' blob:` — for `pdf.worker.mjs`.

**Native window** (`tauri.conf.json:21–30`):
- 1200×800 default, 1000×700 minimum.
- `dragDropEnabled: false` — Tauri's native drag-drop disabled; the app handles drops in JS via `app/src/contexts/DropZoneContext.tsx` + `useFileDrop`.

## Bandwidth gate (cross-cutting integration boundary)

- Singleton `BandwidthManager` registered via `app.manage` in `lib.rs:55` (`app/src-tauri/src/bandwidth.rs`).
- Hard cap: **250 GB/day** (`bandwidth.rs:51`).
- Persisted to `<app_data_dir>/bandwidth.json` (`up_bytes`, `down_bytes`, `date`). Auto-reset at local-midnight rollover (`check_and_reset`).
- Every Telegram up/download path calls `can_transfer(size)` first then `add_up`/`add_down` after — see `commands/fs.rs::cmd_upload_file`, `cmd_download_file`, `commands/preview.rs::cmd_get_preview`.

---

*Integration audit: 2026-04-29*
