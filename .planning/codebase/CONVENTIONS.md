# Coding Conventions

**Analysis Date:** 2026-04-29

This codebase has **no enforced linter or formatter config** (no `.prettierrc`, no
`eslint.config.*`, no `rustfmt.toml`, no `clippy.toml`). Style is enforced by
convention plus the strict checks built into `tsc` and `cargo clippy`. Two
distinct sub-codebases coexist:

- **Rust backend** under `app/src-tauri/src/` — Tauri 2 commands, Actix-web
  streaming server, grammers Telegram client wrapper.
- **TypeScript / React frontend** under `app/src/` — React 19, hooks-only,
  Tanstack Query for server state, Tailwind v4 for styling.

The two halves communicate via Tauri IPC (`invoke` / `#[tauri::command]`) and
two custom Tauri events (`upload-progress`, `download-progress`).

---

## Rust Conventions

### Naming

**Tauri commands:** all prefixed `cmd_`, snake_case, returning `Result<T, String>`.
- `cmd_connect`, `cmd_logout`, `cmd_get_files`, `cmd_upload_file`,
  `cmd_download_file`, `cmd_get_preview`, `cmd_get_thumbnail`,
  `cmd_get_stream_token`, `cmd_is_network_available`, `cmd_clean_cache`,
  `cmd_log` (frontend log relay).
- Defined in `app/src-tauri/src/commands/{auth,fs,preview,network,streaming,utils}.rs`.
- Registered in one big `tauri::generate_handler![…]` block at
  `app/src-tauri/src/lib.rs:80-103`.

**Models / structs (`app/src-tauri/src/models.rs`):** PascalCase ending in
`Metadata` / `Result` / `State`.
- `FileMetadata`, `FolderMetadata`, `Drive`, `AuthResult`, `AuthState`.
- All derive `Debug, Serialize, Deserialize, Clone`.
- Field names are snake_case in Rust; serde converts on the wire. Frontend
  references the snake_case names directly (e.g. `f.icon_type`,
  `f.created_at`) — there is **no `#[serde(rename_all = "camelCase")]`**.

**Modules:** snake_case file names (`bandwidth.rs`, `server.rs`,
`commands/auth.rs`). Each `commands/*.rs` exposes its public items via
`pub use auth::*;` re-exports in `commands/mod.rs:32-37`.

**Constants:** SCREAMING_SNAKE_CASE.
- `PREVIEW_CACHE_MAX_FILES`, `PREVIEW_CACHE_MAX_TOTAL_BYTES` in
  `app/src-tauri/src/commands/preview.rs:9-10`.

### Error Handling

**Standard pattern:** every `#[tauri::command]` returns `Result<T, String>`.
Errors are stringified at the boundary so they cross IPC cleanly.

**Three idioms in use:**

1. **`.map_err(|e| e.to_string())`** — generic conversion, used for filesystem
   and parse errors (e.g. `commands/auth.rs:64`, `commands/fs.rs:126`).
2. **`.map_err(map_error)`** — the Telegram-aware helper at
   `app/src-tauri/src/commands/utils.rs:36-52`. It detects `FLOOD_WAIT`
   errors emitted by grammers and reformats them as `FLOOD_WAIT_<seconds>`
   so the frontend can parse them (`AuthWizard.tsx:135-144` reads this).
   **Use `map_error` for any grammers `client.invoke(...)` /
   `client.request_login_code(...)` / `client.send_message(...)` call.**
3. **`.ok_or("…")`** — for `Option` → `Result` conversion with literal
   strings (e.g. `commands/auth.rs:259`).

**`map_error` MUST be preferred over `e.to_string()`** for any error path that
might surface a flood-wait, because the frontend's flood-wait countdown UI
depends on the `FLOOD_WAIT_<n>` shape.

**Logging:** `log::info!`, `log::warn!`, `log::error!`, `log::debug!` — never
`println!` except the legacy `[Bandwidth]` print in
`app/src-tauri/src/bandwidth.rs:59` (kept for visibility on day-rollover).
`env_logger::init()` is called once in `lib.rs:28`. **Never use `println!` /
`eprintln!` in new code.** Frontend logs are forwarded into the same logger
via `cmd_log` (`commands/utils.rs:26-29`) which prefixes them with
`[FRONTEND]`.

### Mutex Strategy (Critical)

The codebase deliberately mixes `tokio::sync::Mutex` and `std::sync::Mutex`.
This is **not an inconsistency** — each has a specific role. See
`commands/mod.rs:6-23` for the canonical example.

| Mutex type            | Use when                                                                 | Examples                                                          |
|-----------------------|--------------------------------------------------------------------------|-------------------------------------------------------------------|
| `tokio::sync::Mutex`  | Held across `.await` points inside async commands                        | `client`, `login_token`, `password_token`, `api_id` in `TelegramState` |
| `std::sync::Mutex`    | Locked from synchronous contexts (e.g. `RunEvent::Exit` handler)         | `runner_shutdown`, `BandwidthManager.stats`, `ActixServerHandle` |

**Hard rule:** if a value needs to be touched from `RunEvent::Exit` (the
synchronous shutdown handler at `lib.rs:107-129`) it MUST be in a
`std::sync::Mutex`. That's why `runner_shutdown` is `Arc<std::sync::Mutex<…>>`
even though it lives inside a struct full of tokio mutexes.

**`Arc<Mutex<Option<T>>>` is the universal shared-state shape** for anything
that's lazily initialized or replaceable: client, tokens, server handle.
`take()` / `*guard = Some(…)` / `*guard = None` is how state transitions are
expressed.

**Lock scope:** keep guards as short as possible. The pattern at
`commands/auth.rs:33-45` is canonical — drop the guard before any `.await`,
even for std mutexes:

```rust
let did_shutdown_old_runner = {
    let mut guard = state.runner_shutdown.lock().unwrap();
    if let Some(shutdown_tx) = guard.take() {
        let _ = shutdown_tx.send(());
        true
    } else {
        false
    }
}; // MutexGuard dropped here — before the await
if did_shutdown_old_runner {
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

### Mock-Mode Pattern

Every command that touches the Telegram client opens with the same dance
(`commands/fs.rs:14-30` is the template):

```rust
let client_opt = { state.client.lock().await.clone() };
if client_opt.is_none() {
    log::info!("[MOCK] …");
    return Ok(/* mock value */);
}
let client = client_opt.unwrap();
```

Two important details:
1. **Lock + clone + drop** — never hold the client lock across the rest of
   the command. `Client` is cheap to clone (it's an Arc internally).
2. **`[MOCK]` log prefix** — every mock branch emits a log line so it's
   visible during UI iteration without a real Telegram session.

When adding new commands that touch the client, follow this exact shape.

### Async / Spawning

- `tauri::async_runtime::spawn(...)` for tasks that should outlive the
  command (the grammers network runner at `commands/auth.rs:87`).
- `tokio::task::spawn_blocking(...)` for sync syscalls inside async commands
  (`commands/network.rs:12` for the synchronous `TcpStream::connect_timeout`).
- Actix-web is run on its own dedicated `std::thread` with `actix_rt::System`
  because Actix needs its own runtime (`lib.rs:63-76`).

### File / Module Organization

- One responsibility per file under `commands/`: `auth`, `fs`, `preview`,
  `network`, `streaming`, `utils`.
- `mod.rs` only contains: the `TelegramState` struct, `pub mod` declarations,
  and `pub use` re-exports — **no business logic**.
- Cross-cutting helpers (`resolve_peer`, `map_error`) live in
  `commands/utils.rs`.

### Comment Style

Doc comments (`///`) on public items that have non-obvious lifecycle rules.
**The runner-shutdown invariant gets prose-quality comments** —
see `commands/mod.rs:7-10` and `commands/auth.rs:18-19`. Inline `//` comments
explain *why* (not what). Examples:

- `commands/auth.rs:42` — `// MutexGuard dropped here — before the await`
- `commands/auth.rs:31` — `// CRITICAL: Shutdown existing runner before creating a new one`
- `lib.rs:124-126` — explains why the `stop()` future isn't awaited

---

## TypeScript / React Conventions

### Naming

**Files:**
- React components: `PascalCase.tsx` (`AuthWizard.tsx`, `Dashboard.tsx`,
  `FileCard.tsx`).
- Hooks: `useXxx.ts` in `app/src/hooks/` — every file exports exactly one
  hook of the same name (`useFileUpload`, `useFileDownload`,
  `useTelegramConnection`, `useUpdateCheck`, `useNetworkStatus`,
  `useKeyboardShortcuts`, `useFileOperations`, `useFileDrop`).
- Contexts: `XxxContext.tsx`. Note the **historical split**:
  `app/src/context/` (singular) holds `ConfirmContext`, `ThemeContext`;
  `app/src/contexts/` (plural) holds `DropZoneContext`. Both are real, both
  are imported. **Don't try to consolidate without a follow-up cleanup
  commit** — the two paths are referenced from `App.tsx:11-13`.
- Utility / type modules: lowercase (`utils.ts`, `types.ts`).

**Symbols:**
- Components: PascalCase exported function declarations (`export function Dashboard(...)`).
- Hooks: camelCase prefix `use` (`useFileUpload`).
- Event handlers: `handleXxx` (`handleLogout`, `handlePreview`,
  `handleFileClick`).
- Boolean state: `isXxx` / `showXxx` (`isConnected`, `isSyncing`,
  `showMoveModal`, `showHelp`).
- Refs: `xxxRef` (`internalDragRef`, `cancelledRef`, `parentRef`).

**Types / Interfaces (`app/src/types.ts`):**
- PascalCase, no `I`-prefix (`TelegramFile`, `TelegramFolder`, `QueueItem`,
  `BandwidthStats`, `DownloadItem`).
- Field names are camelCase **on the TS side**, even when they map to
  snake_case on the Rust side: e.g. `QueueItem.folderId` (TS) vs
  `cmd_upload_file({folderId})` which Tauri's IPC silently maps to Rust's
  `folder_id` parameter via its built-in serde rename. This works because
  Tauri's IPC layer auto-camel/snake-cases command argument names.
- **Backend-shaped objects keep snake_case** when the frontend reads them
  raw, e.g. `BandwidthStats.up_bytes` / `down_bytes`, and `f.icon_type` /
  `f.created_at` from `cmd_get_files` (`Dashboard.tsx:79`).

### Imports

No path aliases configured. All imports are relative (`./`, `../`, `../../`).

**Conventional grouping** (observed in most files, e.g.
`Dashboard.tsx:1-29`):

1. React + react ecosystem (`react`, `framer-motion`,
   `@tanstack/react-query`).
2. Tauri APIs (`@tauri-apps/api/core`, `@tauri-apps/plugin-*`).
3. Third-party UI (`sonner`, `lucide-react`).
4. Local types (`../types`) and utils (`../utils`).
5. Local components (`./X`, `../components/X`).
6. Local hooks (`../hooks/useX`).

`tsconfig.json` uses `"jsx": "react-jsx"`, so no `import React` needed in
component files (only in `main.tsx` which uses `React.StrictMode`).

### TypeScript Strictness

`tsconfig.json` enables:
- `"strict": true`
- `"noUnusedLocals": true`
- `"noUnusedParameters": true`
- `"noFallthroughCasesInSwitch": true`

These are the **only enforced lint rules** for the frontend. They run as
part of `npm run build` (`tsc && vite build`).

`as any` casts are heavily discouraged — CHANGELOG 1.0.4 documents a sweep
that removed them all. **Two known exceptions** remain (treat as tech debt,
not as patterns to copy):

- `app/src/components/Dashboard.tsx:76` — `invoke<any[]>('cmd_get_files', …)`.
  Should be `invoke<FileMetadata[]>` once the backend serde shape is
  imported into the frontend.
- `app/src/components/dashboard/FileCard.tsx:89` —
  `onDragStart={(e: any) => {...}}` for framer-motion's drag event whose
  type doesn't expose `dataTransfer` cleanly.
- `app/src/components/dashboard/MoveToFolderModal.tsx:32` —
  `folders.map((f: any) => {...})` — should be `(f: TelegramFolder)`.

For `unknown` errors caught from `invoke`, the conventional widen-and-test
pattern is:

```typescript
} catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    setError(msg);
}
```

(`useUpdateCheck.ts:40-47`, `AuthWizard.tsx:133-145`).

### Hooks Architecture

The frontend is **100% hooks-based** — only one class component exists, the
`ErrorBoundary` (which it has to be, because React still requires class
components for `getDerivedStateFromError`).

**Per-hook responsibilities** (single-responsibility, kept under ~225 lines):
- `useTelegramConnection` — folder list, login, logout, sync, store
  bootstrap. Uses dual-store fallback (`config.json` → `settings.json`).
- `useFileUpload` / `useFileDownload` — queue state + IPC progress event
  listener + persistence to the Tauri store.
- `useFileOperations` — bulk delete/download/move handlers (no internal
  state; pure orchestration).
- `useUpdateCheck` — auto-updater state machine (5s startup delay).
- `useNetworkStatus` — 10s TCP-ping poll via `cmd_is_network_available`.
- `useKeyboardShortcuts` — single global keydown listener with disable flag
  for modal contexts.

**Standard hook layout** (e.g. `useFileUpload.ts`):
1. State (`useState`).
2. Refs for cancellation flags (`cancelledRef`).
3. Effect: subscribe to Tauri events with `listen<T>(...)`, return cleanup
   that calls `unlisten?.()`.
4. Effect: load persisted state from store on mount (gate with
   `initialized` flag to avoid re-loading).
5. Effect: persist state changes to store.
6. Effect: queue processor (find next pending → process).
7. Local helpers (`processItem`).
8. Return object with handlers + state.

**React Query keying:** files are keyed by `['files', activeFolderId]`
(`Dashboard.tsx:75`); bandwidth is `['bandwidth']` with
`refetchInterval: 5000` (`Dashboard.tsx:88-93`). Mutations elsewhere
manually call `queryClient.invalidateQueries({ queryKey: ['files', folderId] })`
(`useFileUpload.ts:72`, `useFileOperations.ts:42`, etc.).

### Provider Stack

Order matters and is set in `app/src/App.tsx:43-57`:

```
ErrorBoundary → ThemeProvider → QueryClientProvider → ConfirmProvider → DropZoneProvider → AppContent
```

`useTheme` is consumed before `Toaster` is rendered (sets toast theme).
`ConfirmProvider` and `useConfirm` use a Promise-returning imperative API:

```typescript
const { confirm } = useConfirm();
if (!await confirm({ title, message, variant: 'danger' })) return;
```

(`ConfirmContext.tsx:22-28`, used at e.g. `useFileOperations.ts:17`).
**Always use `confirm()` instead of `window.confirm()`** for user-facing
dialogs. The one exception is `useTelegramConnection.ts:49`, where
`window.confirm` is used as a literal blocker during init before
`ConfirmProvider` is fully wired — don't replicate that.

### Error Handling

**Frontend errors flow through `sonner` toasts**, not browser dialogs. Pattern:

```typescript
try {
    await invoke('cmd_xyz', {...});
    toast.success("Done");
} catch (e) {
    toast.error(`Failed: ${e}`);
}
```

The Tauri `Result::Err(String)` becomes the JS rejection value directly.
Use template-literal `${e}` for display, `String(e)` when storing in state.

**`ErrorBoundary` is the last-resort net** — it logs via `console.error`
(the **only** place `console.*` is allowed; CHANGELOG 1.0.4 documents
removing 16 stray `console.log` / `console.error` calls and the policy that
the `ErrorBoundary` one stays). Do not add `console.log` to ship.

**Exception:** `app/src/components/dashboard/PdfViewer.tsx` has 4 unavoidable
`console.error` calls for pdf.js render/load failures where there's no
toast UI surface (lines 36, 73, 281, 319). These were grandfathered in.

### Styling

- **Tailwind v4** via `@tailwindcss/postcss` and `@theme` directive in
  `app/src/App.css:1-26`.
- Custom palette via CSS vars: `telegram-bg`, `telegram-surface`,
  `telegram-primary` (`#ffae00` dark / `#e69500` light), `telegram-text`,
  `telegram-subtext`, `telegram-border`, `telegram-hover`.
- Theme toggle adds `.light` class to `<html>`; `:root.light` block in
  `App.css` overrides the vars.
- **No CSS Modules, no CSS-in-JS.** Inline styles only for dynamically
  computed dimensions (e.g. virtualizer row positions in `FileExplorer.tsx`
  and explicit pixel heights on `FileCard.tsx:101`). The pixel-height-not-
  aspect-ratio rule is load-bearing — see CHANGELOG 1.0.4.

### Animations

`framer-motion` is the standard animation lib. Patterns:
- `<motion.div>` / `<motion.form>` with `initial` / `animate` / `exit`
  props for mount/unmount transitions.
- Wrap conditionally rendered modals in `<AnimatePresence>` (e.g.
  `Dashboard.tsx:352-387`).
- `whileHover={{ y: -4 }}` for card hover lift in `FileCard.tsx:97`.

Tailwind's `animate-in` / `animate-pulse` / `animate-spin` are used for
simple, non-orchestrated CSS animations (e.g. spinners, status dots).

### Function Design

- **Component files:** typically 100–500 lines. The largest are
  `Dashboard.tsx` (471), `AuthWizard.tsx` (526), and `PdfViewer.tsx` (~440)
  — these orchestrate many sub-features and are an accepted exception to
  "small components".
- **Hooks:** kept under 230 lines (`useTelegramConnection.ts` = 226).
- **Props are typed via `interface XxxProps {…}`** declared at the top of
  the file (e.g. `FileCardProps`, `SidebarProps`).
- **Callback handlers prefer `useCallback`** when passed deep down or used
  in a `useEffect` dep array (`Dashboard.tsx:106-143`).
- **`useMemo` for derived data** when it feeds the virtualizer
  (`FileExplorer.tsx:80-115`).

### Module Exports

- Components: named exports (`export function FileCard(...)`).
- Hooks: named exports.
- `App.tsx` is the only `export default` (because `main.tsx` does
  `import App from './App'`).
- **No barrel files** (`index.ts`). Every import goes to the file that
  defines the symbol.

---

## Cross-Cutting

### Tauri IPC Argument Convention

Frontend calls **camelCase**: `invoke('cmd_upload_file', { folderId, transferId })`.
Backend signature uses **snake_case**: `pub async fn cmd_upload_file(folder_id: Option<i64>, transfer_id: Option<String>, …)`.
Tauri's IPC layer handles the conversion. Don't fight it — use the
language-native casing on each side.

### Versioning

The version number lives in **three places** that must stay in sync:
1. `app/package.json` (`version`)
2. `app/src-tauri/Cargo.toml` (`[package].version`)
3. `app/src-tauri/tauri.conf.json` (`version`)

CLAUDE.md flags this. Bump all three together.

### File Headers

No license headers, no copyright stamps, no `@author` blocks. Keep it that way.

---

*Convention analysis: 2026-04-29*
