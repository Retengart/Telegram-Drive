# Technology Stack

**Analysis Date:** 2026-04-29

## Languages

**Primary:**
- Rust (edition 2021) — Tauri backend, Telegram client, streaming HTTP server. All sources under `app/src-tauri/src/`.
- TypeScript ~5.8.3 — React frontend. All sources under `app/src/`. Strict mode on (`app/tsconfig.json` — `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`).

**Secondary:**
- Bash — release pipeline post-build patcher embedded in `.github/workflows/release.yml` (the `AppRun` heredoc, lines ~163–230).
- HTML/CSS — `app/index.html` entry, `app/src/App.css`, Tailwind utility classes throughout `*.tsx`.

## Runtime

**Environment:**
- Tauri 2 desktop shell (WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows).
- Node.js 20 in CI (`.github/workflows/release.yml` line 73: `node-version: 20`). README requires Node v18+ for local dev.
- Rust stable toolchain in CI (`dtolnay/rust-toolchain@stable`, line 76 of release.yml).
- Two embedded Rust async runtimes — see ARCHITECTURE: `tokio` (full features) for Tauri commands, `actix-rt` for the streaming HTTP server.

**Package Manager:**
- npm — `app/package-lock.json` committed (no yarn/pnpm lockfile present).
- Cargo — `app/src-tauri/Cargo.lock` committed.

## Frameworks

**Core (Rust backend):**
- `tauri` 2 with `tauri-build` 2 — application shell, IPC, window management. Configured in `app/src-tauri/tauri.conf.json`.
- `actix-web` 4 + `actix-cors` 0.7 + `actix-rt` 2 — local HTTP streaming server on `127.0.0.1:14200` (`app/src-tauri/src/server.rs`).
- `tokio` 1 (`features = ["full"]`) — async runtime for Tauri command handlers.
- `grammers-client` / `grammers-session` / `grammers-mtsender` / `grammers-tl-types` — Telegram MTProto client. **All four crates pinned to git rev `d07f96f`** of `https://github.com/Lonami/grammers` (`app/src-tauri/Cargo.toml` lines 23–26). Not on crates.io.

**Core (frontend):**
- React 19.1 + React DOM 19.1 — UI runtime (`app/src/main.tsx`).
- `@tanstack/react-query` ^5.90 — server-state cache for file lists; provider in `app/src/App.tsx`.
- `@tanstack/react-virtual` ^3.13 — virtualised file grid/list (`app/src/components/dashboard/FileExplorer.tsx`).
- `framer-motion` ^12.26 — modal / banner animations.
- `sonner` ^2.0.7 — toast notifications (`<Toaster />` mounted in `app/src/App.tsx`).
- `lucide-react` ^0.562 — icon set (used across all dashboard components).
- `pdfjs-dist` ^5.6.205 — in-app PDF rendering, legacy build (`pdfjs-dist/legacy/build/pdf.mjs` + worker URL via `?url` Vite import in `app/src/components/dashboard/PdfViewer.tsx`).

**Styling:**
- Tailwind CSS 4 + `@tailwindcss/postcss` — configured via `app/postcss.config.js`. No `tailwind.config.*` (Tailwind 4 zero-config / CSS-first).
- `autoprefixer` 10 — PostCSS plugin.

**Testing:**
- Not applicable. No test runner declared. `CLAUDE.md`: "No test suite exists."

**Build/Dev:**
- Vite 7 + `@vitejs/plugin-react` 4.6 — frontend bundler. Dev server pinned to `:1420` (`app/vite.config.ts`).
- TypeScript compiler (`tsc`) — type-check inside `npm run build` (`app/package.json`: `"build": "tsc && vite build"`).
- `tauri-build` 2 (build dep) — generates Tauri bindings (`app/src-tauri/build.rs`).
- `tauri-action@v0` — wraps `tauri build` in CI (`.github/workflows/release.yml` line 100).

## Key Dependencies

**Critical (Rust):**
- `grammers-client` git `d07f96f` — Telegram MTProto. Login flow, file up/download, channel CRUD, message search. Used in `app/src-tauri/src/commands/auth.rs`, `fs.rs`, `preview.rs`, `server.rs`.
- `grammers-session` git `d07f96f` — `SqliteSession` persistence at `<app_data_dir>/telegram.session`.
- `grammers-mtsender` git `d07f96f` — `SenderPool` and the network runner spawned via `tauri::async_runtime::spawn` in `commands/auth.rs`.
- `grammers-tl-types` git `d07f96f` — raw MTProto type/function bindings (`tl::functions::channels::CreateChannel`, `tl::functions::messages::SearchGlobal`, etc.).

**Critical (frontend):**
- `@tauri-apps/api` ^2 — IPC core (`invoke`, `listen`, `convertFileSrc`).
- `@tauri-apps/plugin-store` ^2.4.2 — JSON key-value store. Loads `config.json` then falls back to `settings.json` (`app/src/hooks/useTelegramConnection.ts`).
- `@tauri-apps/plugin-updater` ^2.10.0 — auto-update client (`app/src/hooks/useUpdateCheck.ts`).
- `@tauri-apps/plugin-process` ^2.3.1 — `relaunch()` after update install.
- `@tauri-apps/plugin-dialog` ^2.6.0 — native file/folder pickers.
- `@tauri-apps/plugin-shell` ^2.3.5 — `open()` external URLs (e.g. `my.telegram.org`).

**Infrastructure (Rust):**
- `serde` 1 (`derive` feature) + `serde_json` 1 — IPC payloads, model serialization (`app/src-tauri/src/models.rs`), bandwidth persistence.
- `chrono` 0.4 — local-date stamping for daily bandwidth reset (`app/src-tauri/src/bandwidth.rs`).
- `base64` 0.21 — image preview/thumbnail data-URL encoding (`commands/preview.rs`).
- `log` 0.4 + `env_logger` 0.11 — logging. Initialized once in `lib.rs::run`.
- `rand` 0.8 — 32-char hex stream-token generator (`lib.rs::generate_stream_token`).
- `futures` 0.3 + `async-stream` 0.3 — chunked streaming response in `server.rs`.

**Tauri plugins (Rust side):**
- `tauri-plugin-opener` 2, `tauri-plugin-store` 2, `tauri-plugin-window-state` 2, `tauri-plugin-shell` 2, `tauri-plugin-dialog` 2.6.0, `tauri-plugin-fs` 2, `tauri-plugin-updater` 2.9.0, `tauri-plugin-process` 2.3.1. All registered in `app/src-tauri/src/lib.rs::run`.

## Configuration

**Environment:**
- No `.env` consumed at runtime. Telegram `api_id` / `api_hash` are entered by the user in `AuthWizard`, stored via `tauri-plugin-store` in `<app_data_dir>/config.json` (legacy: `settings.json`).
- CI secrets (`.github/workflows/release.yml`): `GITHUB_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Linux GPU env var set in-process before Tauri builder: `WEBKIT_DISABLE_DMABUF_RENDERER=1` (`app/src-tauri/src/main.rs` lines 9–14).

**Build:**
- `app/src-tauri/tauri.conf.json` — productName, version, identifier `com.cameronamer.telegramdrive`, window dims, CSP, capabilities, updater endpoint + Ed25519 pubkey, bundle targets `"all"`.
- `app/src-tauri/Cargo.toml` — Rust deps and crate name.
- `app/package.json` — frontend deps and scripts.
- `app/vite.config.ts` — Vite dev server pinned `port: 1420`, `strictPort: true`, ignores `**/src-tauri/**` from watch.
- `app/tsconfig.json` — strict TS, ESNext modules, bundler resolution, `react-jsx` transform.
- `app/postcss.config.js` — `@tailwindcss/postcss` + `autoprefixer`.
- `app/src-tauri/capabilities/default.json` — Tauri capability allowlist (see INTEGRATIONS.md "OS surfaces").
- **Version triple-sync:** `app/package.json`, `app/src-tauri/Cargo.toml`, `app/src-tauri/tauri.conf.json` must all be bumped together (CLAUDE.md). Currently: package.json @ 1.1.2 (lagging), Cargo.toml @ 1.1.6, tauri.conf.json @ 1.1.6.

## Platform Requirements

**Development:**
- Node.js v18+ (README); CI uses 20.
- Rust stable toolchain (rustup).
- A Telegram account + `api_id`/`api_hash` from `https://my.telegram.org`.
- Linux build deps (per CI): `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libssl-dev`, `libfuse2`, plus `build-essential`, `curl`, `wget`, `file`.

**Production / bundle targets** (`tauri.conf.json` `bundle.targets: "all"`, matrix in `release.yml`):
- Windows (`windows-latest`) — MSI/NSIS via tauri-action.
- macOS Intel — `--target x86_64-apple-darwin`.
- macOS Apple Silicon — `--target aarch64-apple-darwin`.
- Linux — `ubuntu-22.04` runner; AppImage post-build patched to strip bundled Mesa/EGL/GLVND libs and replace `AppRun` with a host-GPU-preferring wrapper (release.yml lines 110–250). Required for Arch / rolling-distro EGL_BAD_ALLOC fix.

---

*Stack analysis: 2026-04-29*
