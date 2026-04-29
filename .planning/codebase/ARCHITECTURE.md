<!-- refreshed: 2026-04-29 -->
# Architecture

**Analysis Date:** 2026-04-29

## System Overview

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                        Tauri 2 WebView (single window)                  │
│  React 19 + TS frontend  ─  Vite dev server `:1420`  ─  tauri.conf.json │
│  `app/src/main.tsx` → `App.tsx` (provider stack)                        │
│                                                                         │
│   ErrorBoundary → ThemeProvider → QueryClientProvider →                 │
│   ConfirmProvider → DropZoneProvider → AppContent                       │
│                                                                         │
│   AuthWizard  ──or──  Dashboard ──orchestrates──> hooks + dashboard/*   │
└──────────────────────┬───────────────────────────────────┬──────────────┘
                       │ invoke()/listen() over IPC        │ <video src="http://localhost:14200/...">
                       ▼                                   │
┌──────────────────────────────────────────────┐   ┌───────▼───────────────────────┐
│   Rust Runtime A: tauri::Builder + tokio     │   │  Rust Runtime B: Actix-web    │
│   `app/src-tauri/src/lib.rs:27`              │   │  on dedicated `std::thread`   │
│                                              │   │  with `actix_rt::System`      │
│   #[tauri::command] handlers in              │   │  `app/src-tauri/src/server.rs`│
│   `commands/{auth,fs,preview,network,        │   │  `127.0.0.1:14200`            │
│    streaming,utils}.rs`                      │   │  GET /stream/{folder}/{msg}   │
│                                              │   │       ?token=<32 hex>         │
│   Managed state (`app.manage`):              │   │  Streams chunks via           │
│    • TelegramState                           │   │  `client.iter_download(...)`  │
│    • BandwidthManager                        │   │                               │
│    • StreamToken                             │   │  Reads same TelegramState     │
│    • ActixServerHandle                       │   │  via Arc clone from setup     │
└──────────────┬───────────────────────────────┘   └───────────┬───────────────────┘
               │                                               │
               ▼                                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│   `grammers-client` (Lonami/grammers, git rev d07f96f) — Telegram MTProto│
│   `Client`, `SenderPool`, `SqliteSession` (`<app_data_dir>/telegram.session`)│
│   Background "runner" task: `tauri::async_runtime::spawn` + select!{    │
│       runner.run(),                                                     │
│       shutdown_rx (oneshot)   ← stored in TelegramState.runner_shutdown │
│   }                                                                     │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   ▼
                        Telegram DCs (149.154.x.x:443)
```

Disk side-channels:
- `<app_data_dir>/telegram.session{,-wal,-shm}` — SqliteSession, owned by grammers
- `<app_data_dir>/bandwidth.json` — BandwidthManager persistence
- `<app_data_dir>/thumbnails/<msg_id>.<ext>` — inline grid thumbnails (no prune)
- `<app_cache_dir>/previews/<folder|home>_<msg_id>.<ext>` — preview cache (LRU 30 files / 80 MB)
- Frontend stores via `@tauri-apps/plugin-store`: `config.json` (primary) → `settings.json` (legacy fallback)
- `localStorage['theme']` — frontend-only

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| `app_lib::run` | Builds Tauri app, registers plugins/commands/state, spawns Actix thread, drives shutdown via `RunEvent::Exit` | `app/src-tauri/src/lib.rs:27` |
| `main` | Sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Linux, then calls `app_lib::run` | `app/src-tauri/src/main.rs:4` |
| `TelegramState` | Owns `Client`, login/password tokens, runner shutdown oneshot, runner counter | `app/src-tauri/src/commands/mod.rs:11` |
| `ensure_client_initialized` | Singleton-init of grammers `Client`; shuts down old runner before spawning new | `app/src-tauri/src/commands/auth.rs:20` |
| Auth commands | Code request, sign-in, 2FA, connect, check-connection, logout | `app/src-tauri/src/commands/auth.rs` |
| Filesystem commands | Folder CRUD (`channels.CreateChannel`/`DeleteChannel`), upload/download/delete/move/list/search | `app/src-tauri/src/commands/fs.rs` |
| Preview commands | Preview download + base64 image inline + thumbnail cache | `app/src-tauri/src/commands/preview.rs` |
| Streaming token | Returns per-launch 32-hex token to frontend for Actix auth | `app/src-tauri/src/commands/streaming.rs:8` |
| Network probe | Lightweight TCP probe to Telegram DC2 (no grammers) | `app/src-tauri/src/commands/network.rs` |
| `resolve_peer` | Resolves `Option<i64> folder_id` → grammers `Peer` (None = `me`/Saved Messages) | `app/src-tauri/src/commands/utils.rs:6` |
| `map_error` | Parses `FLOOD_WAIT_<n>` from grammers errors | `app/src-tauri/src/commands/utils.rs:36` |
| `start_server` | Binds Actix to `127.0.0.1:14200`, registers `/stream/{folder}/{msg}` with CORS | `app/src-tauri/src/server.rs:104` |
| `stream_media` | Per-request: validates token, resolves peer, fetches msg, streams `iter_download` chunks | `app/src-tauri/src/server.rs:20` |
| `BandwidthManager` | 250 GB/day cap, daily reset, persisted to `bandwidth.json` | `app/src-tauri/src/bandwidth.rs:25` |
| `App.tsx` | Provider stack, gates `AuthWizard` vs `Dashboard` on `isAuthenticated` | `app/src/App.tsx:43` |
| `useTelegramConnection` | Loads store, calls `cmd_connect`, owns folders + `activeFolderId`, sync/create/delete folders | `app/src/hooks/useTelegramConnection.ts:10` |
| `Dashboard` | React Query for files/bandwidth, search debounce (>2 chars), preview routing, drag-drop, modal stack | `app/src/components/Dashboard.tsx:30` |
| `useFileUpload` / `useFileDownload` | Persistent FIFO queues (one in-flight at a time), progress event listeners, cancellation via `cancelledRef` | `app/src/hooks/useFileUpload.ts`, `app/src/hooks/useFileDownload.ts` |
| `useUpdateCheck` | Uses `tauri-plugin-updater` against pinned GH endpoint | `app/src/hooks/useUpdateCheck.ts` |

## Pattern Overview

**Overall:** Tauri-style "thin renderer + Rust core" desktop app. Within Rust, a **dual-runtime split**: tokio for IPC commands and an isolated `actix_rt::System` thread for HTTP media streaming. Within frontend, **provider-wrapped React with hook-orchestrated single-page Dashboard**.

**Key Characteristics:**
- Two Rust async runtimes intentionally isolated (Actix and Tauri/tokio do not share an executor; they share state via `Arc<TelegramState>`).
- Single grammers `Client` per app process; reconnect = swap state and respawn runner (with mandatory shutdown of the old runner first).
- Server-Sent-style progress: Rust commands `app_handle.emit("upload-progress" | "download-progress", ProgressPayload { id, percent })`; frontend `listen()`s and updates queue rows by `id`.
- "Folders" are Telegram channels marked by ` [TD]` title suffix or `[telegram-drive-folder]` about-marker. `null` folder ID == Saved Messages == `me`.
- Streaming server consumed by `<video>` / `<audio>` / PDF viewer using `http://localhost:14200/...?token=...`. CSP in `tauri.conf.json:32` whitelists exactly `connect-src` and `media-src` for `http://localhost:14200`.
- **Mock mode**: when `TelegramState.client` is `None`, six `cmd_*` handlers (`cmd_create_folder`, `cmd_delete_folder`, `cmd_upload_file`, `cmd_download_file`, `cmd_move_files`, `cmd_get_files`) silently return mock/no-op results so the UI runs without auth.

## Layers

**Frontend — React shell (`app/src/main.tsx`, `app/src/App.tsx`):**
- Purpose: render UI tree, provider initialization, gate auth state
- Location: `app/src/`
- Contains: providers, a single `AppContent` switch on `isAuthenticated`, `<Toaster>`, update banner
- Depends on: hooks + components
- Used by: nothing — top of stack

**Frontend — Hooks (`app/src/hooks/`):**
- Purpose: orchestration & side-effects (Tauri IPC, persistent queues, network probe, store loading)
- Location: `app/src/hooks/`
- Contains: `useTelegramConnection` (folders/state), `useFileUpload`/`useFileDownload` (persistent FIFO + progress events), `useFileOperations` (one-off file ops), `useNetworkStatus` (TCP probe poll), `useUpdateCheck`, `useKeyboardShortcuts`, `useFileDrop` (currently inert; native drag-drop disabled)
- Depends on: `@tauri-apps/api/core`, `@tauri-apps/plugin-store`, `@tanstack/react-query`, `sonner`, contexts
- Used by: `Dashboard` and `AuthWizard`

**Frontend — Components (`app/src/components/`):**
- Purpose: pure-ish UI; receive callbacks/state from `Dashboard`
- Location: `app/src/components/` (top-level orchestrators), `app/src/components/dashboard/` (children)
- Contains: `AuthWizard.tsx` (wizard with steps `setup|phone|code|password`), `Dashboard.tsx` (top-level orchestrator), `ErrorBoundary.tsx`, `UpdateBanner.tsx`, `FileTypeIcon.tsx`, `ThemeToggle.tsx`; nested `dashboard/`: `Sidebar`, `TopBar`, `FileExplorer` (virtualized), `FileCard` / `FileListItem`, `UploadQueue` / `DownloadQueue`, `PreviewModal` / `MediaPlayer` / `PdfViewer`, `MoveToFolderModal`, `ContextMenu`, `DragDropOverlay`, `ExternalDropBlocker`, `EmptyState`, `BandwidthWidget`, `SidebarItem`
- Depends on: hooks, contexts, utility modules (`utils.ts`, `types.ts`)
- Used by: each other (parents → children only); no cross-talk

**Frontend — Contexts (`app/src/context/` and `app/src/contexts/`):**
- Purpose: cross-tree primitives
- Location: `app/src/context/` (Theme, Confirm) **and** `app/src/contexts/` (DropZone) — historical split, both real
- Contains: `ThemeContext.tsx` (light/dark, applies class to `<html>` synchronously to avoid flash), `ConfirmContext.tsx` (Promise-returning `confirm()`), `DropZoneContext.tsx` (currently empty placeholder)
- Used by: hooks and components

**Backend — Tauri command surface (`app/src-tauri/src/commands/`):**
- Purpose: the IPC API exposed via `invoke()`
- Location: `app/src-tauri/src/commands/`
- Contains: 23 commands wired in `lib.rs:80`–`103` covering auth, fs, preview, thumbnails, search, scanning, bandwidth, logging, network probe, cache wipe, stream-token, connection-check
- Depends on: `TelegramState`, `BandwidthManager`, `grammers-*` crates
- Used by: frontend via `invoke('cmd_*', { camelCaseArgs })`

**Backend — Streaming server (`app/src-tauri/src/server.rs`):**
- Purpose: HTTP media chunk streaming for `<video>`/`<audio>`/PDF
- Location: `app/src-tauri/src/server.rs`
- Contains: single Actix route `GET /stream/{folder_id}/{message_id}?token=...`
- Depends on: `actix-web`, `actix-cors`, `actix-rt`, shared `Arc<TelegramState>`, `StreamTokenData`, `async-stream`
- Used by: frontend `<video src="http://localhost:14200/...">` after fetching token via `cmd_get_stream_token`

**Backend — Models (`app/src-tauri/src/models.rs`):**
- Purpose: serde DTOs crossing the IPC boundary
- Contains: `AuthState` (tagged enum), `AuthResult`, `FileMetadata`, `FolderMetadata`, `Drive`

**Backend — Bandwidth (`app/src-tauri/src/bandwidth.rs`):**
- Purpose: 250 GB/day quota gate, JSON-persisted, daily auto-reset
- Used by: `cmd_upload_file`, `cmd_download_file`, `cmd_get_preview` (all call `can_transfer` then `add_up`/`add_down`)

## Data Flow

### Primary Request Path — list files in a folder

1. User clicks folder in `Sidebar` → `setActiveFolderId(id)` (`app/src/hooks/useTelegramConnection.ts:204`, persisted to `config.json`).
2. React Query refetches with key `['files', activeFolderId]` → `invoke('cmd_get_files', { folderId })` (`app/src/components/Dashboard.tsx:74`).
3. `cmd_get_files` (`app/src-tauri/src/commands/fs.rs:295`): clones `Client` from `TelegramState.client`, calls `resolve_peer`, iterates `client.iter_messages(&peer)`, projects each `Media::Document|Photo` to `FileMetadata`.
4. Result deserialized into `TelegramFile[]` and merged with computed `sizeStr` / `type` (`Dashboard.tsx:76-80`).
5. `FileExplorer` virtualizes the list via `@tanstack/react-virtual`; grid `cardHeight = cardWidth * 0.75`, rows then padded to `Math.max(cardHeight + GAP, 150)`px so virtualizer rows match rendered DOM (`app/src/components/dashboard/FileExplorer.tsx:67-72`).

### Upload Flow

1. User clicks "Upload" → `useFileUpload.handleManualUpload` opens `@tauri-apps/plugin-dialog` (`app/src/hooks/useFileUpload.ts:86`).
2. Items appended to `uploadQueue` with `status: 'pending'`. Pending items are persisted to `Store('config.json')` under `uploadQueue`.
3. Effect at `useFileUpload.ts:54` picks the next pending and calls `processItem`. Only one upload runs at a time (`processing` flag).
4. `invoke('cmd_upload_file', { path, folderId, transferId })` (`app/src-tauri/src/commands/fs.rs:117`):
   - `BandwidthManager.can_transfer(size)` gate.
   - Emit `upload-progress { id, percent: 0 }`.
   - `client.upload_file(&path)` (boxed onto `tauri::async_runtime::spawn` to satisfy lifetime constraints).
   - `client.send_message(&peer, InputMessage::new().text("").file(uploaded))`.
   - `BandwidthManager.add_up(size)`; emit `upload-progress { id, percent: 100 }`.
5. Frontend `listen<ProgressPayload>('upload-progress', ...)` updates the matching row's `progress` (`app/src/hooks/useFileUpload.ts:24-32`).
6. On success, `queryClient.invalidateQueries({ queryKey: ['files', folderId] })` triggers a refetch.

### Download Flow

Identical queue topology in `useFileDownload` (`app/src/hooks/useFileDownload.ts`). `cmd_download_file` (`app/src-tauri/src/commands/fs.rs:187`) iterates `client.iter_download(&media)` chunk-by-chunk, writes to disk, computes percent per chunk, emits `download-progress` only when integer percent changes (anti-spam), and `add_down(total_size)` once at completion.

### Streaming Flow (video/audio/PDF)

1. User opens `MediaPlayer` / `PdfViewer`. `useEffect` calls `invoke<string>('cmd_get_stream_token')` (`app/src/components/dashboard/MediaPlayer.tsx:21`).
2. URL constructed: `http://localhost:14200/stream/<folderIdOrHome>/<messageId>?token=<32hex>` — `home` substituted for `null` folder id.
3. `<video>` / `<audio>` / `<embed>` element issues HTTP GET; Actix handler (`server.rs:20`) validates token, resolves peer, fetches the message, opens `client.iter_download(&media)`, returns `HttpResponse::Ok().streaming(...)` with `Content-Type` from doc mime, `Content-Length`, `Cache-Control: private, max-age=120`.
4. Token is regenerated every app launch (`generate_stream_token` in `lib.rs:16`); URLs from prior runs are dead.

### Auth Flow (`AuthWizard`)

1. `setup`: user enters API ID + API hash, saved to `Store('config.json')`. `cmd_auth_request_code(phone, api_id, api_hash)` → grammers `request_login_code`. Retries up to 2× on `AUTH_RESTART`/500. Returns `code_sent`.
2. `code`: `cmd_auth_sign_in(code)` → grammers `sign_in(login_token, code)`. On `SignInError::PasswordRequired` → stash password token, return `next_step: "password"`.
3. `password`: `cmd_auth_check_password(password)` → grammers `check_password(token, password)`. Returns `next_step: "dashboard"`.
4. `useTelegramConnection.initStore` on app start: reads `api_id` from store, calls `cmd_connect({ apiId })`, which calls `ensure_client_initialized`. On failure, prompts retry-or-logout.

### Connection Recovery

- `useNetworkStatus` polls `cmd_is_network_available` every 10 s (`app/src/hooks/useNetworkStatus.ts:33`); pure TCP probe to `149.154.167.50:443` (Telegram DC2) on a `tokio::task::spawn_blocking`, 2 s timeout. Avoids any grammers calls deliberately (network-down with grammers blew the stack).
- `cmd_check_connection` (`commands/auth.rs:117`): pings via `client.get_me()`. On failure, clears `state.client`, calls `ensure_client_initialized` with cached `api_id`, pings again.

### Shutdown (`RunEvent::Exit`) — `lib.rs:107-128`

1. Take `runner_shutdown` oneshot from `TelegramState`; `tx.send(())` → `tokio::select!` in the runner task takes the shutdown branch and drops the runner.
2. Take `ServerHandle` from `ActixServerHandle`; `handle.stop(true)` (`drop`-the-future, do not await — we are in a synchronous Tauri callback).
3. Without these, terminal Ctrl+C hangs (history: v1.1.6).

**State Management:**
- Backend: all mutable shared state inside `Arc<Mutex<...>>` fields of `TelegramState`. **`runner_shutdown` is a `std::sync::Mutex`** (not tokio) so it can be locked from the synchronous `RunEvent::Exit` handler.
- Frontend: React Query for server-derived data (`['files', folderId]`, `['bandwidth']`); local `useState` for UI; persistent queues + folders + viewMode + activeFolderId in `@tauri-apps/plugin-store` (`config.json`); theme in `localStorage`.

## Key Abstractions

**`TelegramState` (`app/src-tauri/src/commands/mod.rs:11`):**
- Purpose: app-wide singleton bundling grammers client + login state + runner shutdown
- Pattern: `Clone`-able struct of `Arc<Mutex<...>>` fields, registered with `app.manage(...)` so all Tauri commands and the Actix server (via cloned `Arc`) share it

**`BandwidthManager` (`app/src-tauri/src/bandwidth.rs:25`):**
- Purpose: thread-safe daily-reset bandwidth ledger backed by a JSON file
- Pattern: `app.manage(BandwidthManager::new(handle))` singleton; `can_transfer(size)` gate before every transfer, `add_up`/`add_down` after

**`StreamToken` / `StreamTokenData` (`commands/streaming.rs:5`, `server.rs:9`):**
- Purpose: per-launch shared secret authenticating the streaming server against frontend
- Pattern: same hex string registered as managed state on Tauri side and as `web::Data` on Actix side

**`ActixServerHandle` (`lib.rs:24`):**
- Purpose: lets the Tauri exit handler (synchronous) stop the Actix server (which lives on another thread/runtime)
- Pattern: `Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>>`; populated after `start_server(...)` resolves, drained on `RunEvent::Exit`

**`useFileUpload` / `useFileDownload` queues:**
- Purpose: persistent serial work queues with cancel + restore-on-startup
- Pattern: state machine `pending → uploading|downloading → success|error|cancelled`; only `pending` items persisted; cancellation via `Set<id>` ref consulted after `invoke` resolves to suppress success/error toast
- Files: `app/src/hooks/useFileUpload.ts`, `app/src/hooks/useFileDownload.ts`

**Folder = Channel:**
- Purpose: storage abstraction
- Pattern: each "folder" is a private Telegram broadcast channel created with title `"<name> [TD]"` and about `"...\n[telegram-drive-folder]"`, then `messages.SetHistoryTtl { period: 0 }` to disable auto-deletion. Discovery in `cmd_scan_folders` matches title first, falls back to about-marker.

## Entry Points

**Process entry (`app/src-tauri/src/main.rs`):**
- Location: `app/src-tauri/src/main.rs:4`
- Triggers: OS launcher / `npm run tauri dev|build`
- Responsibilities: set `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Linux **before** any GTK/WebKit init, then call `app_lib::run`

**Tauri builder (`app/src-tauri/src/lib.rs`):**
- Location: `app/src-tauri/src/lib.rs:27`
- Triggers: `app_lib::run()`
- Responsibilities: register plugins (`opener`, `store`, `shell`, `dialog`, `fs`, `updater`, `process`, `window-state`), generate stream token, manage state, spawn streaming server thread, wire 23 `invoke_handler` commands, drive `RunEvent::Exit` shutdown

**Frontend bootstrap (`app/src/main.tsx`):**
- Location: `app/src/main.tsx:5`
- Triggers: `index.html` script tag / Vite dev server
- Responsibilities: `ReactDOM.createRoot(...)` → `<React.StrictMode><App/></React.StrictMode>`

**Streaming server thread (`lib.rs:63`):**
- Location: `app/src-tauri/src/lib.rs:63` → `app/src-tauri/src/server.rs:104`
- Triggers: `setup` callback during Tauri builder
- Responsibilities: own dedicated `actix_rt::System`, bind `127.0.0.1:14200`, register `stream_media`, stash `ServerHandle` for shutdown

**IPC commands:** registered in `lib.rs:80-103`. Each `cmd_*` handler is its own callable entry from JS via `invoke('cmd_*', { camelCaseArgs })`. Tauri auto-converts snake_case Rust params to camelCase on the JS side (`folder_id` → `folderId`, etc.).

## Architectural Constraints

- **Threading:** Two long-lived runtimes. Tauri runs on tokio (`#[tokio::main]`-equivalent inside `tauri::Builder`). Actix runs on its own dedicated `std::thread` because `actix_rt::System` requires its own executor and cannot be hosted inside tokio. They share state only through `Arc<TelegramState>` and `Arc<Mutex<Option<ServerHandle>>>`.
- **Grammers runner lifecycle (CRITICAL):** Before spawning a new network runner you MUST signal the old one via `runner_shutdown`'s oneshot, `sleep(100ms)`, then create a new pool. Skipping this leaks runner tasks and exhausts the thread stack — see comment block at `app/src-tauri/src/commands/auth.rs:18-44`. `runner_count` (`AtomicU32`) is logged for debugging.
- **`runner_shutdown` is `std::sync::Mutex`, not `tokio::sync::Mutex`.** It must be lockable from the synchronous `RunEvent::Exit` callback (`lib.rs:113`).
- **Single Actix port (14200) hardcoded.** Conflicts on this port = no streaming. Not configurable.
- **Tauri native drag-drop is disabled** (`tauri.conf.json:28`, `dragDropEnabled: false`). All drag-drop is plain DOM events; `useFileDrop` is therefore inert. `ExternalDropBlocker` swallows external drops to channel users into the file picker.
- **CSP is locked** (`tauri.conf.json:32`) to `connect-src 'self' http://localhost:14200; media-src 'self' http://localhost:14200`. Adding new outbound HTTP needs a CSP edit.
- **`WEBKIT_DISABLE_DMABUF_RENDERER=1`** must be set before `tauri::Builder` runs on Linux. The AppImage `AppRun` wrapper is the bundled half; this env var is the in-process half.
- **AppImage post-build patching**: CI strips bundled Mesa/EGL libs and replaces `AppRun`. Don't bundle Mesa/GL libs — they get ripped out anyway.
- **Global state inventory:**
  - `TelegramState` (managed) — `Arc<Mutex<Option<Client>>>` and friends
  - `BandwidthManager` (managed) — internal `Mutex<BandwidthStats>` + `PathBuf`
  - `StreamToken` (managed)
  - `ActixServerHandle` (managed) — `Arc<std::sync::Mutex<Option<ServerHandle>>>`
- **Version triplet must stay in sync** (per `CLAUDE.md`): `app/package.json:version`, `app/src-tauri/Cargo.toml:version`, `app/src-tauri/tauri.conf.json:version`.

## Anti-Patterns

### Spawning a grammers runner without first shutting down the previous one

**What happens:** Code calls `SenderPool::new(...)` and `tauri::async_runtime::spawn(runner.run())` without consulting `state.runner_shutdown`.
**Why it's wrong:** Runner tasks accumulate; each carries grammers' large reconnect future; the thread stack overflows. Multiple commands referenced this in their original design.
**Do this instead:** Always go through `ensure_client_initialized` (`app/src-tauri/src/commands/auth.rs:20`), which `take()`s the existing oneshot, sends `()`, sleeps 100 ms, then sets up the new runner with a fresh oneshot.

### Using `tokio::sync::Mutex` for state touched by `RunEvent::Exit`

**What happens:** A field of `TelegramState` is wrapped in `tokio::sync::Mutex` instead of `std::sync::Mutex`.
**Why it's wrong:** `RunEvent::Exit` runs in a synchronous Tauri context — `tokio::sync::Mutex::lock` is `async`. You'd have to block on a runtime that's already shutting down.
**Do this instead:** `runner_shutdown` and `ActixServerHandle` are both `std::sync::Mutex`. Match this for any new exit-time state.

### Calling grammers from the network probe

**What happens:** `cmd_is_network_available` uses `client.get_me()` or any other grammers method to check connectivity.
**Why it's wrong:** When the network is genuinely down, grammers' reconnect logic blew the stack (see comment in `app/src-tauri/src/commands/network.rs:1-7`).
**Do this instead:** Raw `TcpStream::connect_timeout` to a hardcoded Telegram DC IP on `tokio::task::spawn_blocking`, 2 s timeout.

### Awaiting `ServerHandle::stop` from `RunEvent::Exit`

**What happens:** `handle.stop(true).await` in a synchronous handler.
**Why it's wrong:** Synchronous handler; awaiting a future requires a runtime that may be torn down.
**Do this instead:** `drop(handle.stop(true))` — sending the stop signal is synchronous; the returned future tracks drain completion, which we don't need on exit (`lib.rs:126`).

### Adding Mesa/EGL libs to the bundle config

**What happens:** Mesa or libEGL listed under `tauri.conf.json` `bundle.resources` or similar.
**Why it's wrong:** CI's AppImage post-process strips them and replaces `AppRun` to use the host stack. They get ripped out anyway, and bloat the bundle.
**Do this instead:** Rely on `WEBKIT_DISABLE_DMABUF_RENDERER=1` (`main.rs:11`) plus the AppImage CI patch.

### Setting grid card heights via CSS `aspect-ratio` only

**What happens:** Card uses `aspect-[4/3]` and the virtualizer estimates row height.
**Why it's wrong:** Virtualizer's row-height calc desyncs from rendered DOM → blank rows / overlap (CHANGELOG 1.0.4).
**Do this instead:** Compute `cardHeight = cardWidth * 0.75` and pass `rowHeight = Math.max(cardHeight + GAP, 150)` to `useVirtualizer` (`FileExplorer.tsx:67-72`).

### Reading the `contexts/` (plural) and `context/` (singular) directories as duplicates

**What happens:** Code consolidates them or assumes one is dead.
**Why it's wrong:** Both are real. `context/` has Theme + Confirm; `contexts/` has DropZone. Provider stack pulls from both. Renaming one breaks `App.tsx` imports.
**Do this instead:** Leave the split as-is unless explicitly refactoring; keep DropZone in `contexts/` and Theme/Confirm in `context/`.

## Error Handling

**Strategy:** Rust `Result<T, String>` across IPC, mapped on the frontend to `toast.error(...)` or React Query `error` state. `map_error` (`commands/utils.rs:36`) extracts `FLOOD_WAIT_<seconds>` from grammers errors so the frontend can show a countdown.

**Patterns:**
- Sign-in special case: `SignInError::PasswordRequired(token)` is not an error; it stashes the token and returns `AuthResult { next_step: "password" }` (`commands/auth.rs:274`).
- Session corruption: on `SqliteSession::open` failure, delete `telegram.session{,-wal,-shm}` and reopen (`commands/auth.rs:64`).
- Auth restart: 2 retries on `AUTH_RESTART` / `500` for `request_login_code` (`commands/auth.rs:223`).
- Bandwidth gate: returns `Result<(), String>` with a human-readable cap message; callers `?`-bubble it.
- React `ErrorBoundary` wraps the whole app (`App.tsx:45`) catching uncaught render errors.
- Network heuristic in `useTelegramConnection.isNetworkError` (`app/src/hooks/useTelegramConnection.ts:77`) matches keywords (`timeout`, `connection`, `EOF`, `overflow`, ...) to decide between retry vs. force-logout.

## Cross-Cutting Concerns

**Logging:**
- Backend: `env_logger::init()` in `lib.rs:28`. Use `log::info!` / `log::warn!` / `log::error!` / `log::debug!`. Frontend can post log lines via `cmd_log` (`commands/utils.rs:27`) which prefixes with `[FRONTEND]`.
- Frontend: `console.*` plus `sonner` toasts for user-facing surface.

**Validation:**
- Backend: ad-hoc per command (e.g. `api_hash.trim().is_empty()` in `commands/auth.rs:209`, `folder_id_str` parsing in `server.rs:38`).
- Frontend: form-level only; `searchTerm.length > 2` gates `cmd_search_global` (`Dashboard.tsx:84,168`).

**Authentication:**
- IPC: implicit (Tauri sandbox; only the local WebView can `invoke`).
- Streaming server: 32-hex token query param, regenerated per app launch, validated against `StreamTokenData.token` (`server.rs:27`); rejects with `403 Forbidden`.

**Bandwidth gating:**
- Every transfer path (`cmd_upload_file`, `cmd_download_file`, `cmd_get_preview`) calls `bw.can_transfer(size)?` before, and `bw.add_up(size)` / `bw.add_down(size)` after. Hardcoded 250 GB/day, midnight local reset.

**Caching:**
- Preview cache pruned LRU by mtime in `prune_preview_cache` (`commands/preview.rs:12`) on every `cmd_get_preview` call.
- Thumbnail cache (`<app_data_dir>/thumbnails/<msg_id>.<ext>`): no prune; assumed small, 1 file per message id.
- `cmd_clean_cache` wipes only `previews/`; called on logout from `useTelegramConnection.handleLogout` (`app/src/hooks/useTelegramConnection.ts:105`).

---

*Architecture analysis: 2026-04-29*
