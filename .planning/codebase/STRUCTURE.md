# Codebase Structure

**Analysis Date:** 2026-04-29

## Directory Layout

```
Telegram-Drive/
├── .github/
│   └── workflows/
│       ├── main.yml          # CI checks (presumed; not inspected)
│       └── release.yml       # Tag-triggered release; AppImage post-build patch
├── .planning/
│   └── codebase/             # This directory: GSD codebase mapper output
├── screenshots/              # README assets
├── CHANGELOG.md              # Per-version release notes
├── CLAUDE.md                 # Project instructions for Claude Code
├── README.md                 # Project README
└── app/                      # Everything compilable lives here
    ├── index.html            # Vite HTML entry; loads /src/main.tsx
    ├── package.json          # Frontend deps + scripts (`dev`, `build`, `tauri`)
    ├── package-lock.json
    ├── tsconfig.json         # TS config (frontend)
    ├── tsconfig.node.json    # TS config (vite.config.ts)
    ├── vite.config.ts        # Port 1420, ignores src-tauri/**, react plugin
    ├── postcss.config.js     # tailwindcss + autoprefixer
    ├── public/               # Static assets served at root
    ├── test_upload.txt       # Sample file for manual upload testing
    ├── src/                  # Frontend (React 19 + TS)
    │   ├── main.tsx              # ReactDOM.createRoot → <App/>
    │   ├── App.tsx               # Provider stack + auth gate
    │   ├── App.css               # Global styles + telegram-* CSS vars
    │   ├── types.ts              # TelegramFile / TelegramFolder / QueueItem / DownloadItem / BandwidthStats
    │   ├── utils.ts              # formatBytes, isMediaFile/isVideoFile/isAudioFile/isImageFile/isPdfFile
    │   ├── vite-env.d.ts
    │   ├── assets/               # logo.svg, react.svg
    │   ├── components/
    │   │   ├── AuthWizard.tsx        # 4-step wizard: setup/phone/code/password
    │   │   ├── Dashboard.tsx         # Top-level orchestrator (post-login)
    │   │   ├── ErrorBoundary.tsx     # React error boundary at root
    │   │   ├── FileTypeIcon.tsx      # Lucide-icon picker keyed by extension
    │   │   ├── ThemeToggle.tsx
    │   │   ├── UpdateBanner.tsx      # Shown when useUpdateCheck reports update
    │   │   └── dashboard/            # Dashboard children (one component per file)
    │   │       ├── BandwidthWidget.tsx
    │   │       ├── ContextMenu.tsx
    │   │       ├── DownloadQueue.tsx
    │   │       ├── DragDropOverlay.tsx
    │   │       ├── EmptyState.tsx
    │   │       ├── ExternalDropBlocker.tsx
    │   │       ├── FileCard.tsx
    │   │       ├── FileExplorer.tsx     # Virtualized list/grid
    │   │       ├── FileListItem.tsx
    │   │       ├── MediaPlayer.tsx      # <video>/<audio> against streaming server
    │   │       ├── MoveToFolderModal.tsx
    │   │       ├── PdfViewer.tsx        # pdfjs-dist
    │   │       ├── PreviewModal.tsx
    │   │       ├── SidebarItem.tsx
    │   │       ├── Sidebar.tsx
    │   │       ├── TopBar.tsx
    │   │       └── UploadQueue.tsx
    │   ├── context/              # Theme + Confirm contexts (singular form)
    │   │   ├── ConfirmContext.tsx    # Promise-returning confirm()
    │   │   └── ThemeContext.tsx      # light/dark, applies <html class>
    │   ├── contexts/             # DropZone context (plural form — historical split)
    │   │   └── DropZoneContext.tsx   # Currently a placeholder (empty interface)
    │   └── hooks/
    │       ├── useFileDownload.ts        # Persistent download queue + progress events
    │       ├── useFileDrop.ts            # Stub; native drag-drop disabled
    │       ├── useFileOperations.ts      # One-off bulk ops (delete/download/move)
    │       ├── useFileUpload.ts          # Persistent upload queue + progress events
    │       ├── useKeyboardShortcuts.ts   # Cmd/Ctrl+A/F, Delete, Esc, Enter
    │       ├── useNetworkStatus.ts       # 10 s poll of cmd_is_network_available
    │       ├── useTelegramConnection.ts  # Store load + folders + connect/logout/sync
    │       └── useUpdateCheck.ts         # tauri-plugin-updater wrapper
    └── src-tauri/            # Backend (Rust)
        ├── build.rs                  # `tauri_build::build()` (one-liner)
        ├── Cargo.toml                # Crate `app`, lib name `app_lib`
        ├── Cargo.lock
        ├── tauri.conf.json           # productName, identifier, CSP, updater pubkey, version
        ├── capabilities/
        │   └── default.json              # Tauri capability set granted to the WebView
        ├── icons/                    # Bundle icons (png/icns/ico/iconset)
        └── src/
            ├── main.rs                   # Linux EGL workaround → app_lib::run()
            ├── lib.rs                    # tauri::Builder, plugins, state, Actix thread, RunEvent::Exit
            ├── models.rs                 # serde DTOs crossing IPC
            ├── bandwidth.rs              # 250 GB/day quota (BandwidthManager)
            ├── server.rs                 # Actix streaming server (port 14200)
            └── commands/
                ├── mod.rs                    # TelegramState struct + sub-module re-exports
                ├── auth.rs                   # ensure_client_initialized + auth/connect/logout commands
                ├── fs.rs                    # Folder/file CRUD, list, search, move, upload, download
                ├── network.rs                # cmd_is_network_available (TCP probe)
                ├── preview.rs                # cmd_get_preview, cmd_get_thumbnail, cmd_clean_cache
                ├── streaming.rs              # StreamToken state + cmd_get_stream_token
                └── utils.rs                  # resolve_peer, map_error, cmd_log, cmd_get_bandwidth
```

## Directory Purposes

**`app/` (build root):**
- Purpose: everything Tauri/Vite needs lives under here
- Contains: frontend (`src/`), backend (`src-tauri/`), `package.json`, `vite.config.ts`, HTML entry
- Key files: `app/package.json`, `app/vite.config.ts`, `app/src-tauri/tauri.conf.json`, `app/src-tauri/Cargo.toml`

**`app/src/`:**
- Purpose: React 19 + TypeScript frontend
- Contains: components, hooks, contexts, type definitions, utility helpers
- Key files: `app/src/main.tsx`, `app/src/App.tsx`, `app/src/types.ts`, `app/src/utils.ts`

**`app/src/components/`:**
- Purpose: top-level UI (`AuthWizard`, `Dashboard`) + reusable wrappers (`ErrorBoundary`, `UpdateBanner`)
- Contains: `Dashboard.tsx` orchestrates everything post-login; `AuthWizard.tsx` is the pre-login flow

**`app/src/components/dashboard/`:**
- Purpose: children of `Dashboard.tsx`. One component per file. Pure-ish; receive callbacks/state as props.
- Contains: explorer/list components, modals, queue widgets, sidebar/topbar, drop overlays
- Key files: `FileExplorer.tsx` (virtualized via `@tanstack/react-virtual`), `MediaPlayer.tsx` / `PdfViewer.tsx` (consume streaming server)

**`app/src/hooks/`:**
- Purpose: side-effectful logic — Tauri IPC, persistent queues, network polling, keyboard shortcuts
- Contains: `use*.ts` files, one hook each
- Key files: `useTelegramConnection.ts` (folder/connect lifecycle), `useFileUpload.ts` / `useFileDownload.ts` (persistent FIFO queues)

**`app/src/context/` (singular):**
- Purpose: cross-tree primitives — Theme and Confirm
- Contains: `ThemeContext.tsx`, `ConfirmContext.tsx`
- Note: do NOT merge with `contexts/` — both are imported from `App.tsx`

**`app/src/contexts/` (plural):**
- Purpose: DropZone provider
- Contains: `DropZoneContext.tsx` (currently inert; placeholder for future drop-target detection)
- Note: historical split with `context/`. Both must exist — see `App.tsx` imports

**`app/src-tauri/`:**
- Purpose: Rust backend crate (`app_lib`) and Tauri configuration
- Contains: `Cargo.toml`, `tauri.conf.json`, `build.rs`, `capabilities/default.json`, icons
- Key files: `Cargo.toml` (deps incl. `grammers-*` git rev `d07f96f`), `tauri.conf.json` (CSP, updater, window config)

**`app/src-tauri/src/`:**
- Purpose: Rust source
- Contains: top-level `lib.rs` / `main.rs` / `models.rs` / `bandwidth.rs` / `server.rs`, plus `commands/`

**`app/src-tauri/src/commands/`:**
- Purpose: All `#[tauri::command]` IPC handlers, plus `TelegramState`
- Contains: one module per concern — `auth`, `fs`, `preview`, `network`, `streaming`, `utils`
- Key files: `mod.rs` (state struct + glob re-exports), `auth.rs` (the runner-lifecycle gate `ensure_client_initialized`)

**`app/src-tauri/capabilities/`:**
- Purpose: Tauri 2 permission capability files (allow-list of plugins/commands per webview)
- Contains: `default.json`
- Generated: No
- Committed: Yes

**`app/src-tauri/icons/`:**
- Purpose: Bundle icons used by `tauri.conf.json:bundle.icon`
- Generated: No (hand-curated)
- Committed: Yes

**`.github/workflows/`:**
- Purpose: GitHub Actions CI
- Contains: `release.yml` (tag-triggered, builds installers, post-build-patches Linux AppImage to strip Mesa/EGL and rewrite `AppRun`); `main.yml` (general CI)

**`.planning/codebase/`:**
- Purpose: GSD codebase mapper output (these documents)
- Generated: Yes (by `/gsd-map-codebase`)
- Committed: Project decision

## Key File Locations

**Entry Points:**
- `app/src-tauri/src/main.rs:4` — process entry; sets `WEBKIT_DISABLE_DMABUF_RENDERER` then calls `app_lib::run`
- `app/src-tauri/src/lib.rs:27` — Tauri builder, state setup, command registration, exit shutdown
- `app/src/main.tsx:5` — React render root
- `app/src/App.tsx:43` — provider stack + auth gate
- `app/index.html` — Vite HTML entry, loads `/src/main.tsx` as ESM module

**Configuration:**
- `app/src-tauri/tauri.conf.json` — productName, version (must match `Cargo.toml` and `package.json`), CSP, updater endpoint + Ed25519 pubkey, window dims (`1200x800`, min `1000x700`), `dragDropEnabled: false`
- `app/src-tauri/Cargo.toml` — Rust deps including `grammers-*` git rev `d07f96f`, `actix-web`, `actix-cors`, `actix-rt`, `tokio`, `tauri-plugin-*`
- `app/package.json` — frontend deps (`react@19`, `@tanstack/react-query`, `@tanstack/react-virtual`, `framer-motion`, `lucide-react`, `pdfjs-dist`, `sonner`, `tailwindcss@4`, `vite@7`)
- `app/vite.config.ts` — port 1420, ignore `src-tauri/**` from watch, react plugin
- `app/tsconfig.json` / `app/tsconfig.node.json` — TS configs
- `app/postcss.config.js` — tailwind + autoprefixer
- `app/src-tauri/capabilities/default.json` — Tauri permission set
- `.github/workflows/release.yml` — release pipeline (AppImage patch lives here)

**Core Logic — Backend:**
- `app/src-tauri/src/commands/mod.rs:11` — `TelegramState` definition (the central singleton)
- `app/src-tauri/src/commands/auth.rs:20` — `ensure_client_initialized` (runner lifecycle gate)
- `app/src-tauri/src/commands/fs.rs` — folder/file commands, including `cmd_scan_folders` (`:419`), `cmd_create_folder` (`:11`), `cmd_upload_file` (`:117`), `cmd_download_file` (`:187`), `cmd_get_files` (`:295`), `cmd_search_global` (`:333`), `cmd_move_files` (`:264`)
- `app/src-tauri/src/commands/preview.rs:42` — `cmd_get_preview`; `:12` `prune_preview_cache`
- `app/src-tauri/src/server.rs:20` — `stream_media`; `:104` `start_server`
- `app/src-tauri/src/bandwidth.rs:25` — `BandwidthManager`
- `app/src-tauri/src/lib.rs:107` — `RunEvent::Exit` shutdown choreography
- `app/src-tauri/src/lib.rs:16` — `generate_stream_token`

**Core Logic — Frontend:**
- `app/src/components/Dashboard.tsx:30` — orchestrator; React Query, queues, search, modals, virtualization
- `app/src/hooks/useTelegramConnection.ts:10` — store loading, folders, connect/logout
- `app/src/hooks/useFileUpload.ts:16` / `useFileDownload.ts:14` — persistent FIFO queues
- `app/src/components/dashboard/FileExplorer.tsx:32` — `useGridColumns` + virtualizer height computation
- `app/src/components/dashboard/MediaPlayer.tsx:21` — fetches stream token, builds `http://localhost:14200/...` URL
- `app/src/components/AuthWizard.tsx` — auth wizard

**Testing:**
- None. There is no test suite (per `CLAUDE.md`). No `*.test.*` / `*.spec.*` files exist. No CI test step. `app/test_upload.txt` is just a manual-upload sample.

## Naming Conventions

**Files:**
- React components: `PascalCase.tsx` (`Dashboard.tsx`, `AuthWizard.tsx`, `FileExplorer.tsx`)
- Hooks: `useCamelCase.ts` (`useFileUpload.ts`, `useTelegramConnection.ts`)
- Context modules: `PascalCaseContext.tsx` (`ThemeContext.tsx`, `ConfirmContext.tsx`, `DropZoneContext.tsx`)
- TS shared modules: lowercase (`types.ts`, `utils.ts`)
- Rust modules: `snake_case.rs` (`bandwidth.rs`, `server.rs`, `auth.rs`)
- Tauri command functions: `cmd_<verb>_<noun>` (`cmd_upload_file`, `cmd_get_preview`, `cmd_scan_folders`, `cmd_is_network_available`)

**Directories:**
- Frontend dirs: lowercase (`components`, `hooks`, `context`, `contexts`, `assets`)
- Rust dirs: lowercase (`commands`, `capabilities`, `icons`)
- Component subgroups: lowercase nested under `components/` (`components/dashboard/`)

**Variables / functions:**
- Rust: `snake_case` for fields and functions; `PascalCase` for types; `SCREAMING_SNAKE_CASE` for constants (`PREVIEW_CACHE_MAX_FILES`)
- TS/React: `camelCase` for vars/functions, `PascalCase` for components/types
- IPC arg conversion: snake_case Rust params auto-mapped to camelCase JS keys (`folder_id` ↔ `folderId`, `message_id` ↔ `messageId`, `save_path` ↔ `savePath`, `transfer_id` ↔ `transferId`)

**Tauri events:**
- kebab-case strings: `upload-progress`, `download-progress`

## Where to Add New Code

**New `#[tauri::command]` (IPC handler):**
- Choose category file:
  - Auth/session/connection: `app/src-tauri/src/commands/auth.rs`
  - Folder or file CRUD / listing: `app/src-tauri/src/commands/fs.rs`
  - Preview / thumbnail / cache: `app/src-tauri/src/commands/preview.rs`
  - Network probe / connectivity: `app/src-tauri/src/commands/network.rs`
  - Streaming-server-related: `app/src-tauri/src/commands/streaming.rs`
  - Generic helper / logging / bandwidth: `app/src-tauri/src/commands/utils.rs`
  - If none fits: add a new sub-module under `commands/`, declare it in `commands/mod.rs:25-30`, add `pub use ...::*;` re-export below.
- Define as `pub async fn cmd_<verb>_<noun>(... State<'_, TelegramState>, ...)` returning `Result<T, String>`.
- Wire it in `app/src-tauri/src/lib.rs:80-103` `tauri::generate_handler![...]` list.
- Bandwidth-touching commands MUST gate on `BandwidthManager::can_transfer` and finalize via `add_up`/`add_down`.
- If async + uses grammers, route through the existing `Client` clone pattern (`let client_opt = state.client.lock().await.clone();`) and respect mock mode (`if client_opt.is_none() { ... }`).

**New React component:**
- Top-level (used directly from `App.tsx`): `app/src/components/<Name>.tsx`
- Dashboard child: `app/src/components/dashboard/<Name>.tsx`. Wire it into `Dashboard.tsx`.
- Pure presentational: takes props only; no IPC. Side-effects belong in hooks.

**New hook:**
- `app/src/hooks/use<Name>.ts`. Single named export `use<Name>`. Call `invoke()` for IPC and `listen()` for events here, not in components.

**New persistent state:**
- Use `@tauri-apps/plugin-store` via the `store` from `useTelegramConnection`. Save under a dedicated key (`store.set('myKey', value)` then `store.save()`). Read on mount with `store.get<T>('myKey')`. Both `config.json` and the legacy `settings.json` are tried.

**New Telegram-side state on the Rust side:**
- Add fields to `TelegramState` in `app/src-tauri/src/commands/mod.rs:11` and initialize them in the `app.manage(TelegramState { ... })` call at `lib.rs:47`. Use `Arc<tokio::sync::Mutex<...>>` for async-locked state, **`Arc<std::sync::Mutex<...>>` only if it must be touched from `RunEvent::Exit`**.

**New event from Rust to frontend:**
- `app_handle.emit("<kebab-case-event>", payload)` from a `#[tauri::command]`. Frontend: `listen<Payload>('<kebab-case-event>', ...)` in a hook's `useEffect`, return the `unlisten` fn. Mirror the existing `upload-progress` / `download-progress` pattern.

**New endpoint on the streaming server:**
- Add `#[get("/...")]` handler in `app/src-tauri/src/server.rs`. Validate `query.token == token_data.token`. Register with `.service(...)` inside the `HttpServer::new(...)` closure (`server.rs:118-122`). Update `tauri.conf.json:32` CSP if a new origin is needed (currently `http://localhost:14200` is whitelisted).

**New CSS / theme token:**
- Tailwind config is implicit (`@tailwindcss/postcss` v4). Theme palette uses `text-telegram-*` / `bg-telegram-*` / `border-telegram-*` utility classes — these are wired via CSS variables in `app/src/App.css`. Light/dark flip is via `<html class="light">` / `<html class="dark">` set by `ThemeContext`.

**New file type recognition:**
- Add to the `*_EXTENSIONS` arrays in `app/src/utils.ts:12-15` and the `match` blocks in `app/src-tauri/src/commands/preview.rs:80-95` (preview ext fallback) and `app/src/components/FileTypeIcon.tsx`.

**New version release:**
- Bump in three places (per `CLAUDE.md`): `app/package.json:version`, `app/src-tauri/Cargo.toml:version`, `app/src-tauri/tauri.conf.json:version`. Tag `vX.Y.Z`, push tag, CI takes over.

## Special Directories

**`app/.npm-cache/`:**
- Purpose: local npm cache (project-scoped)
- Generated: Yes
- Committed: No (`.gitignore`d)

**`app/src-tauri/target/`:**
- Purpose: Rust build artifacts
- Generated: Yes
- Committed: No

**`app/dist/`:**
- Purpose: Vite build output, fed to Tauri as `frontendDist` (`tauri.conf.json:18`)
- Generated: Yes (by `npm run build`)
- Committed: No

**`app/src-tauri/icons/`:**
- Purpose: bundle icons
- Generated: No (hand-curated)
- Committed: Yes

**`screenshots/`:**
- Purpose: README marketing screenshots
- Generated: No
- Committed: Yes

**`.planning/codebase/`:**
- Purpose: GSD codebase mapper output (these documents)
- Generated: Yes
- Committed: Project decision

---

*Structure analysis: 2026-04-29*
