# Codebase Concerns

**Analysis Date:** 2026-04-29

> **Threat model context.** This app stores a full Telegram MTProto session
> (`<app_data_dir>/telegram.session`) on the user's machine. Anyone who can read
> that file or run code in the app's WebView can take over the user's Telegram
> account. Anyone who can hit `127.0.0.1:14200` with the runtime token can
> exfiltrate any media from any chat the user has access to. Most CRITICAL items
> below trace back to that single trust boundary.

---

## Tech Debt

### Three-way version drift between `package.json`, `Cargo.toml`, `tauri.conf.json`

- **Severity:** MEDIUM
- Files: `app/package.json:4` (`"version": "1.1.2"`), `app/src-tauri/Cargo.toml:3` (`version = "1.1.6"`), `app/src-tauri/tauri.conf.json:12` (`"version": "1.1.6"`)
- Impact: `CLAUDE.md` mandates these stay in sync. They are not. Updater manifests are keyed off `tauri.conf.json` so the user-facing version is fine, but `package.json` is a documented release-bump touchpoint that drifted in 1.1.3+.
- Fix: bump `app/package.json` to `1.1.6` and add a CI check (e.g., `node -e "require('assert').strictEqual(require('./app/package.json').version, '<tag>')"`).

### `cmd_search_global` Messages vs Slice copy-paste

- **Severity:** LOW
- Files: `app/src-tauri/src/commands/fs.rs:361-413` (Messages branch lines 361-386, Slice branch lines 387-413 — identical bodies)
- Impact: Bug fixes and schema updates have to be applied twice; one branch will eventually drift.
- Fix: extract `fn extract_files_from_messages(msgs: Vec<tl::enums::Message>, files: &mut Vec<FileMetadata>)` and call it from both arms; or destructure `let msgs = match result { Messages(m) => m.messages, Slice(m) => m.messages, _ => return Ok(Vec::new()) };`.

### Module split between `context/` and `contexts/`

- **Severity:** LOW
- Files: `app/src/context/ConfirmContext.tsx`, `app/src/context/ThemeContext.tsx`, `app/src/contexts/DropZoneContext.tsx`
- Impact: Confusing import paths; new contributors will guess wrong half the time. Documented in `CLAUDE.md` as "historical split."
- Fix: consolidate into one directory, update imports.

### Frontend `any` usage

- **Severity:** LOW
- Files: `app/src/components/Dashboard.tsx:76` (`invoke<any[]>`), `app/src/components/dashboard/MoveToFolderModal.tsx:32` (`folders.map((f: any)`), `app/src/components/dashboard/FileCard.tsx:89` (`onDragStart={(e: any)`)
- Impact: Bypasses TypeScript checking; CHANGELOG 1.0.4 claimed all `as any` casts were removed but at least three `any` annotations remain.
- Fix: type `cmd_get_files` return as `FileMetadata[]`, type `folders: TelegramFolder[]`, use `React.DragEvent<HTMLDivElement>`.

### `cmd_get_files` does an unbounded `iter_messages` walk

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/fs.rs:309-327`
- Impact: Every folder open iterates the entire channel history with no `limit`, no `MessagesFilter`, no pagination. For a Saved Messages with 50k items this is slow and rude to Telegram's API (FLOOD_WAIT risk). Contrast with `cmd_search_global` which hard-codes `limit: 50` (`fs.rs:354`).
- Fix: add `.filter(MessagesFilter::Document)` if available and an explicit `.limit(N)` with cursor-based loading (frontend already React-Query'd by `activeFolderId`, easy to add `pageParam`).

### `cmd_create_folder` silently ignores `SetHistoryTtl` failure

- **Severity:** LOW
- Files: `app/src-tauri/src/commands/fs.rs:62-65` (`let _ = client.invoke(...)`)
- Impact: If the TTL-disable call fails (rate-limit / transient error) the new folder retains Telegram's default TTL and silently deletes user data later. User has no warning.
- Fix: log the error at minimum; ideally retry once before returning success.

### Mock-mode coupling makes prod bugs invisible

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/fs.rs:20-28, 83-86, 132-136, 174-178, 199-203, 270-275, 300-303` and `auth.rs` paths
- Impact: When `client.is_none()` we return `Ok(...)` mock results from destructive commands. There is no compile-time gate (e.g., `#[cfg(debug_assertions)]`); a release build with a corrupt session that fails to initialize will appear to "succeed" deletes/uploads against a non-existent backend.
- Fix: gate mock branches behind `cfg!(debug_assertions)` or a dedicated `--features mock-mode` flag.

---

## Known Bugs

### Cancel-all uploads/downloads doesn't cancel the in-flight Rust task

- **Severity:** MEDIUM
- Symptoms: User clicks "Cancel All", UI says cancelled, but the active `cmd_upload_file` / `cmd_download_file` keeps running until the underlying `grammers` call completes — bandwidth still counts, file still arrives, error toast then fires.
- Files: `app/src/hooks/useFileUpload.ts:105-114`, `app/src/hooks/useFileDownload.ts:136-145`, `app/src-tauri/src/commands/fs.rs:117-166` (no abort-token plumbing in `cmd_upload_file`/`cmd_download_file`)
- Trigger: cancel during a multi-MB transfer.
- Fix: introduce a per-`transfer_id` `CancellationToken` (`tokio_util::sync::CancellationToken`) stored in `TelegramState`, check it inside the `iter_download` loop in `fs.rs:238-251` and inside the upload spawn at `fs.rs:147-150`.

### `useTelegramConnection` infinite-reload loop on connect failure

- **Severity:** MEDIUM
- Symptoms: When `cmd_connect` throws and user clicks "Retry" in the native confirm, `window.location.reload()` is called — which re-runs `initStore`, which immediately tries `cmd_connect` again with the same broken state. Loop until user clicks Cancel.
- Files: `app/src/hooks/useTelegramConnection.ts:48-57`
- Trigger: corrupt session that survives the auth-side recreation logic, or expired API ID.
- Fix: on retry, first call `cmd_logout` to reset state, then reload; or just re-run `ensure_client_initialized` without a full page reload.

### `handleBulkDownload` / `handleDownloadFolder` builds paths with hardcoded `/`

- **Severity:** MEDIUM
- Symptoms: On Windows the constructed path is `C:\Users\foo\Downloads/file.bin` — works by accident on most APIs but breaks on a few legacy ones, and breaks toast messages.
- Files: `app/src/hooks/useFileOperations.ts:73, 116`
- Fix: use Tauri's `path.join` (`@tauri-apps/api/path`) or at least detect platform separator.

### `handleBulkDownload` swallows per-file errors silently

- **Severity:** LOW
- Files: `app/src/hooks/useFileOperations.ts:77, 120` (`catch (e) { }`)
- Impact: A folder of 100 files with 30 failures shows "Downloaded 70 files" with no indication anything went wrong.
- Fix: collect failures, surface count and first error message.

### Filename written to disk is the Telegram-attribute filename, unsanitized

- **Severity:** HIGH (security-adjacent)
- Symptoms: Telegram allows arbitrary `DocumentAttribute::Filename` strings, including `../../../.bashrc` or `/etc/passwd`. The frontend builds `${dirPath}/${file.name}` and Rust calls `std::fs::File::create(&save_path)` directly. A malicious uploader (or attacker who pre-seeds the user's chat) can write outside the chosen download directory.
- Files: `app/src/hooks/useFileOperations.ts:73, 116`, `app/src/hooks/useFileDownload.ts:122` (frontend builds path), `app/src-tauri/src/commands/fs.rs:234` (`File::create(&save_path)` — no validation)
- Fix: in Rust, normalise `save_path` and assert that, after canonicalisation, it lives under a frontend-supplied "download root" (passed separately, not concatenated with name). Strip path separators from filenames; replace `..` segments. Same applies to preview-cache filename derivation but that uses message_id only so it's safe.

### Bandwidth manager uses local midnight, not session start, for resets

- **Severity:** LOW
- Files: `app/src-tauri/src/bandwidth.rs:55-68`
- Impact: Crossing a DST boundary or system clock change confuses the reset; user can also game the limit by setting clock back. Not security-critical but documented as "Resets at local midnight" — verify behavior matches.
- Fix: store reset epoch alongside `up_bytes`/`down_bytes`; reset when `now > reset_epoch + 24h`.

---

## Security Considerations

### CRITICAL — WebView can read the raw Telegram session via Tauri fs plugin

- **Severity:** CRITICAL
- Files: `app/src-tauri/capabilities/default.json:11-14` grants `fs:default + fs:allow-appdata-read-recursive + fs:allow-appdata-write-recursive`; session at `<app_data_dir>/telegram.session` (created by `app/src-tauri/src/commands/auth.rs:59`); `api_hash` plaintext at `<app_data_dir>/config.json` written by `app/src/components/AuthWizard.tsx:91-100`.
- Risk: Any successful XSS / supply-chain compromise in the frontend (React 19, framer-motion, sonner, lucide-react, pdfjs-dist, react-virtual, react-query, plus all their transitive deps) gives the attacker the full SQLite session file. Telegram session = full account control: read all messages, post as user, take over linked services. Plus the `api_hash` from `my.telegram.org` which is a per-user secret on Telegram's side too.
- Current mitigation: CSP at `tauri.conf.json:32` blocks `unsafe-eval` and external `script-src`, which raises the bar but is bypassed by any DOM-XSS sink in a dependency. `connect-src 'self' http://localhost:14200` is good. No `dangerouslySetInnerHTML` audit visible.
- Fix:
  1. Drop `fs:allow-appdata-read-recursive`/`fs:allow-appdata-write-recursive` — frontend never reads files from app-data directly, all access is via `#[tauri::command]` handlers; verify by `grep -rn "readTextFile\|readFile\|readBinaryFile" app/src/`.
  2. The store plugin already namespaces `config.json` correctly; remove the broad `fs:default` permission (add `fs:allow-app-meta` only if needed).
  3. Encrypt the session at rest (e.g., `keyring` crate / OS keychain wrap of an AES key) so a casual file copy from the data dir doesn't grant takeover.

### CRITICAL — destructive Telegram operations have no `[TD]` scoping

- **Severity:** CRITICAL
- Files:
  - `app/src-tauri/src/commands/fs.rs:74-108` (`cmd_delete_folder` calls `channels::DeleteChannel` on whatever `folder_id` resolves to)
  - `app/src-tauri/src/commands/fs.rs:168-184` (`cmd_delete_file`)
  - `app/src-tauri/src/commands/fs.rs:263-292` (`cmd_move_files` forwards-then-deletes)
  - `app/src-tauri/src/commands/fs.rs:117-166` (`cmd_upload_file`)
- Risk: Backend trusts the frontend-supplied `folder_id`. Any compromised frontend script can delete arbitrary Telegram channels the user owns (work groups, group chats they admin), delete arbitrary messages from any peer including DMs with friends, or upload data to any chat. The whole "folder" abstraction (`[TD]` title / `[telegram-drive-folder]` about-text marker, `cmd_scan_folders` at `fs.rs:419-478`) is enforced only on the *display* side.
- Current mitigation: none. Mock-mode early-return (`fs.rs:20-28`) doesn't help against an authenticated session.
- Fix: in each destructive command, after resolving the peer, fetch the channel's `about` field (or cache the marker on first scan into `TelegramState`) and reject with `Err("not a Telegram Drive folder")` when the marker is absent. For `Saved Messages`, allow only `folder_id == None` (own peer) — not arbitrary user peers.

### CRITICAL — streaming server allows arbitrary media exfiltration

- **Severity:** CRITICAL
- Files: `app/src-tauri/src/server.rs:19-95` (`/stream/{folder_id}/{message_id}?token=...`); token-gen `app/src-tauri/src/lib.rs:15-20`; token retrieval `app/src-tauri/src/commands/streaming.rs:7-10`.
- Risk: The token gates *who can hit the server* but not *what they can request*. With one token (frontend has it always) the caller can request `/stream/<any_channel_id>/<any_message_id>` and stream the bytes of any media from any chat the user is in. Same scoping bug as the destructive commands but exploitable just by reading the URL.
- Token-in-query-string concerns:
  - URL is in browser history / WebView memory dumps.
  - Logged by `actix-web` access logs (default off, but `env_logger` could be turned up).
  - Visible to any JS that can read `window.history` or `performance.getEntriesByType('resource')` — including embedded media-frame `src` introspection if a CSP gap is found.
- Current mitigation: 32-char random token regenerated each launch; bound to `127.0.0.1` so external machines can't reach it; CSP `connect-src` constrains origins.
- Fix:
  1. Apply the same `[TD]`-folder scoping check in `stream_media` before fetching media (look up peer marker / pre-validate folder_id against the cached folder list).
  2. Move token from URL query to `Authorization: Bearer` header; pdfjs supports `httpHeaders`, `<video>` doesn't easily — alternative: short-lived per-message presigned-style tokens (HMAC of `(folder_id, message_id, exp)`).
  3. Add an actix middleware that rate-limits per-token requests.

### HIGH — CORS allows `http://localhost:1420` (dev origin) in release builds

- **Severity:** HIGH
- Files: `app/src-tauri/src/server.rs:111-116`
- Risk: A user who happens to run *any* dev server on `http://localhost:1420` (Vite default) while the desktop app is also running, and visits a malicious site that probes localhost, can — once they obtain a stream token — exfiltrate media. CORS isn't checked when the request is same-origin or when it's a `<video src>` (which is opaque), but it does enable `fetch` from a co-resident dev server. Token is the gate; CORS just removes one layer.
- Current mitigation: token still required.
- Fix: only register `http://localhost:1420` in the allowed origins list when `cfg!(debug_assertions)`. Production should accept only `tauri://localhost` and `https://tauri.localhost`.

### MEDIUM — `cmd_log` is unauthenticated, unrate-limited, and trusts caller content

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/utils.rs:26-29` — `pub fn cmd_log(message: String) { log::info!("[FRONTEND] {}", message); }`
- Risk:
  1. Log injection: caller passes `"\n[ERROR] CRITICAL: master key leaked"` and it appears verbatim in the log stream.
  2. DoS: tight loop calling `cmd_log("x".repeat(1_000_000))` consumes I/O and memory.
  3. Currently has no callers in `app/src/` (search returns zero hits), so the attack surface is unused. It's also gated by Tauri IPC origin checks.
- Current mitigation: Tauri 2 IPC enforces same-origin, so only the WebView can reach it. Still trusts a compromised WebView.
- Fix: drop `\n`/`\r` from messages; cap length to 4096; rate-limit (token bucket per-process). Or remove the command entirely since nothing is using it.

### MEDIUM — `app_data_dir().unwrap()` panic in `cmd_logout`

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/auth.rs:190` (`let app_data_dir = app_handle.path().app_data_dir().unwrap();`)
- Risk: On a misconfigured system (no HOME, no AppData), `cmd_logout` panics inside the IPC worker — the user sees the app freeze instead of a graceful error, and the session file is *not* removed. In a security-sensitive flow this is the wrong failure mode.
- Fix: same `.map_err(|e| format!(...))?` pattern as `auth.rs:51-53`.

### MEDIUM — session file deleted but in-memory state may leak

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/auth.rs:161-198`
- Risk: `cmd_logout` removes `telegram.session{,-wal,-shm}` but the `Client` and SQLite handles inside `grammers` may still hold the auth keys in heap until dropped. A heap dump while the app is paused on the auth screen post-logout could expose them. SQLite WAL contents may also persist in OS page cache.
- Current mitigation: `*state.client.lock().await = None;` drops the Arc, eventually freeing memory.
- Fix: explicitly zeroize sensitive memory (`zeroize` crate); call `client.sign_out()` *before* clearing state so server-side keys are revoked first (current order: lines 168-180 do this, good).

### MEDIUM — `1.1.6` patch ships `WEBKIT_DISABLE_DMABUF_RENDERER=1` permanently on Linux

- **Severity:** LOW (security), MEDIUM (UX)
- Files: `app/src-tauri/src/main.rs:9-14`
- Risk: Falling back from DMA-BUF to in-process compositing increases attack surface for WebKitGTK CVEs that the DMA-BUF path mitigates. Trade-off documented in CLAUDE.md as required for Arch/rolling-distro compat.
- Fix: gate behind a runtime detect (e.g., only set if `LIBGL_ALWAYS_SOFTWARE` is set, or only on first-launch detection of EGL_BAD_ALLOC). Lower priority.

### LOW — `latest.json` updater signature does not cover the patched AppImage

- **Severity:** HIGH (release pipeline correctness, may invalidate updater)
- Files: `.github/workflows/release.yml:99-108` (tauri-action build, generates Ed25519 sig of unpatched AppImage), `release.yml:110-250` (post-build patch step modifies the AppImage but does NOT re-sign)
- Risk: The Ed25519 signature uploaded to the release alongside the AppImage was computed on the un-patched binary. After the patch step rewrites the squashfs and `AppRun`, the bundled `*.AppImage.sig` file matches a *different* binary. When the Tauri auto-updater on a user's machine downloads the patched AppImage and verifies the signature, **verification will fail** and updates silently break — or worse, `tauri-plugin-updater` may have signature-required mode that bricks updates entirely.
- Verify: download the latest AppImage from the GH release, also download `latest.json`, run `minisign -V -P <pubkey> -m <appimage> -x <sig>` — if it fails, this is real.
- Fix: re-run `tauri signer sign` (or `minisign -S`) on the patched AppImage and overwrite the published signature file before the publish-release job. Requires `TAURI_SIGNING_PRIVATE_KEY` to be available in the patch step too.

### LOW — public Telegram API ID/Hash is per-user but trivially extractable

- **Severity:** LOW
- Files: `<app_data_dir>/config.json` (plaintext), set by `app/src/components/AuthWizard.tsx:93-96`
- Risk: User's `api_hash` is the secret half of their Telegram developer credential. Plaintext on disk + CRITICAL #1 above means trivial to grab. Telegram considers this user-private.
- Fix: at minimum, store under OS keychain (`tauri-plugin-stronghold` or platform `keyring`).

---

## Performance Bottlenecks

### `resolve_peer` does an O(N dialogs) scan on every operation

- **Severity:** HIGH
- Files: `app/src-tauri/src/commands/utils.rs:6-24`
- Problem: For every `cmd_get_files`, `cmd_upload_file`, `cmd_download_file`, `cmd_delete_file`, `cmd_move_files`, `cmd_get_preview`, `cmd_get_thumbnail`, *and every streaming-server request*, we call `client.iter_dialogs()` and walk it sequentially until we hit the matching channel id. For a user with 500 dialogs that's 500 protocol round-trips (the iterator pages). This is also called per-thumbnail in a virtualized grid — easily hundreds per scroll.
- Cause: no peer cache; `grammers` peer hashing requires a recent dialog to construct InputPeer.
- Improvement path: build a `HashMap<i64, Peer>` cached in `TelegramState` populated on first `cmd_scan_folders` and refreshed lazily on miss. For unknown ids, fall back to current `iter_dialogs` scan and insert. Add cache invalidation on `cmd_logout`.

### `cmd_get_files` loads the entire channel history per folder switch

- **Severity:** HIGH
- Files: `app/src-tauri/src/commands/fs.rs:295-330`
- Problem: No `limit`, no pagination, no filter. A folder with 10k files re-fetches all 10k on every React-Query re-fetch (which `Dashboard.tsx:74-82` re-keys on `activeFolderId`).
- Improvement path: use `iter_messages(&peer).filter(MessagesFilter::Document).limit(50)` + cursor pagination; expose `cmd_get_files_page(folder_id, offset_id)` and switch frontend to `useInfiniteQuery`.

### Thumbnail download fetches full-resolution photo, not the thumbnail

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/preview.rs:251-269`
- Problem: `client.download_media(&media, ...)` for `Media::Photo` downloads the full original. For `Media::Document` with image MIME, it downloads the entire file (could be a 50MB PNG). The cache is unbounded (`<app_data_dir>/thumbnails/` per CLAUDE.md "no prune").
- Improvement path: use `media.thumbs()` / smallest `PhotoSize` (grammers exposes `Media::Photo::thumbs`) and download the smallest variant. Also add LRU pruning to the thumbnails dir.

### `cmd_scan_folders` does a `GetFullChannel` round-trip per non-`[TD]` channel

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/fs.rs:450-468`
- Problem: For every channel without `[TD]` in the title, the code makes a separate API call to fetch `about`. A user with 200 channels = 200 round-trips on every "Sync" click.
- Improvement path: scan only on first connect with no cached folders; persist channel-id -> is-folder-flag mapping; on subsequent syncs, only check newly seen channels.

### Bandwidth manager writes JSON to disk on every `add_up`/`add_down`

- **Severity:** LOW
- Files: `app/src-tauri/src/bandwidth.rs:80-98`
- Problem: Every byte counter increment triggers a full `serde_json::to_string` + `fs::write` of the whole file. With chunk-streaming downloads we still call `add_down` once per file (not per chunk — verified at `fs.rs:253`), so impact is small. But preview/thumbnail flow can call `add_down` in fast succession.
- Improvement path: debounce writes (write at most every 5s, flush on exit).

---

## Fragile Areas

### Grammers runner lifecycle (verified — claim #7 is real)

- **Severity:** HIGH (reliability)
- Files: `app/src-tauri/src/commands/auth.rs:31-45, 81-98`, `lib.rs:107-128` (Ctrl+C path), `commands/mod.rs:18-22` (the `runner_shutdown` field)
- Why fragile:
  - The `100ms sleep` after sending shutdown (`auth.rs:43-45`) is a guess, not a sync. If the runner takes longer to exit, two runners briefly co-exist.
  - `runner_shutdown` is `Arc<std::sync::Mutex<...>>` (sync mutex) so it can be locked from `RunEvent::Exit` in `lib.rs:113`. This is correct but means *async* code paths must `.lock().unwrap()` synchronously — risk of holding it across an await would deadlock (see `auth.rs:34-42` carefully scoped to drop guard before await — good).
  - Counter (`runner_count`) is debug-only; if it ever wraps it's just confusing logging.
- Test coverage: zero. CLAUDE.md notes "No test suite exists."
- Safe modification: never call `ensure_client_initialized` while the runner_shutdown mutex is held; never `.await` while holding `runner_shutdown`; always drop `client_guard` *before* spawning a runner (currently we hold it through line 100, fine because no awaits between line 83 lock and line 100 store).

### `cmd_check_connection` reconnect path may double-spawn a runner

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/auth.rs:117-158`
- Why fragile: Line 141 sets `client = None`, then line 143 calls `ensure_client_initialized` which (at lines 33-42) signals the *old* runner to shut down. But the old runner was already detached from this client (we just nulled it). The shutdown signal works because `runner_shutdown` is keyed by sender, not by client. Still — if `cmd_check_connection` is called concurrently with itself (two re-fetches racing) you can briefly have two runners.
- Fix: serialise reconnect with a `tokio::sync::Mutex<()>` reconnect-lock.

### `lib.rs:60` clones `TelegramState` to pass to the Actix thread, but its inner `Mutex<Option<Client>>` is shared by Arc

- **Severity:** LOW (works correctly today)
- Files: `app/src-tauri/src/lib.rs:60`, `commands/mod.rs:11-23`
- Why fragile: `TelegramState` derives `Clone`; cloning copies the `Arc`s, so both copies see the same client/login_token state. This is the desired behavior but it's load-bearing and easy to break by adding a non-`Arc` field.
- Fix: enforce by adding a `compile_check` that all fields are `Arc`-like.

### Streaming server `Content-Length` is wrong for `Photo` media

- **Severity:** LOW
- Files: `app/src-tauri/src/server.rs:56-80` — sets `Content-Length: 0` for `Media::Photo` (line 58: `Photo(_) => 0`), then streams arbitrary bytes via `streaming(stream)`.
- Why fragile: Setting an explicit `Content-Length: 0` while streaming bytes is invalid HTTP. Some clients (curl with `--http1.1`, certain video players, pdfjs) will refuse to read the body. Today it's not exercised because preview/stream UIs route photos through `cmd_get_thumbnail`/`cmd_get_preview` (base64), not the streaming server.
- Fix: omit `Content-Length` when unknown; use `Transfer-Encoding: chunked` (actix sets this automatically when `streaming()` is used without explicit length).

### Path-traversal vulnerability in download save_path

- **Severity:** HIGH (security)
- Files: `app/src-tauri/src/commands/fs.rs:189, 234`
- Why fragile: see "Filename written to disk is the Telegram-attribute filename, unsanitized" under Bugs.

### `cmd_search_global` `unwrap()` on `d.document`

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/fs.rs:365, 391` (`if let tl::enums::Document::Document(doc) = d.document.unwrap()`)
- Why fragile: If a server response includes `MessageMedia::Document { document: None }` (DocumentEmpty case), this panics inside the Tauri command, propagating as IPC error and breaking search. Telegram does occasionally return empty document references for deleted media.
- Fix: replace `.unwrap()` with `if let Some(doc_enum) = d.document { ... }`.

### Network-availability check is a single hardcoded IP

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/network.rs:14-17`
- Why fragile: `149.154.167.50` is Telegram DC2. If Telegram rotates DC IPs, or this specific host is blocked by the user's firewall while the rest of the internet works, the app reports "offline." Conversely, if DC2 is reachable but the user's auth DC is down, the app reports "online" but auth fails.
- Fix: try multiple DC IPs (DC1 `149.154.175.50`, DC2 `149.154.167.50`, DC4 `149.154.167.91`) with `tokio::select!`; fall back to a generic connectivity probe if needed.

### Ctrl+C handler doesn't await `client.sign_out()` or save in-flight upload state

- **Severity:** LOW
- Files: `app/src-tauri/src/lib.rs:107-129`
- Why fragile: On Ctrl+C, runner is signalled and Actix is stopped, but in-flight `cmd_upload_file` futures are dropped mid-upload. Telegram doesn't get a clean `.sign_out()`. Acceptable today but if uploads ever resume from byte offsets, this matters.
- Fix: extend the Exit handler to await up to 2s for in-flight transfers to complete.

---

## Privacy

### `cmd_clean_cache` does NOT clear `<app_data_dir>/thumbnails/`

- **Severity:** MEDIUM
- Files: `app/src-tauri/src/commands/preview.rs:161-174` (only deletes `previews` subdir of *cache* dir); thumbnails written to `<app_data_dir>/thumbnails/` at `preview.rs:187-194, 253-258`.
- Risk: After logout, every thumbnail downloaded during the session persists on disk forever. For a user who used the app to browse a sensitive folder then logged out (e.g., on a shared device), the thumbnails of everything they viewed are still readable. Logout flow at `useTelegramConnection.ts:104-111` calls `cmd_clean_cache` but no separate thumbnail-cleanup command exists.
- Verification: confirmed by reading both functions; thumbnail dir is `app_data_dir().join("thumbnails")` (preview.rs:189-191), preview cleanup is `app_cache_dir().join("previews")` (preview.rs:165-169).
- Fix: in `cmd_logout` (auth.rs:160-198) or in a new `cmd_clean_thumbnails`, also `remove_dir_all(<app_data_dir>/thumbnails)`. Add LRU pruning during normal operation too.

### Legacy `settings.json` may retain `api_hash` after upgrade

- **Severity:** LOW–MEDIUM
- Files: `app/src/hooks/useTelegramConnection.ts:25-67` (loads `config.json`, falls back to `settings.json` at line 30); `AuthWizard.tsx:91-100` (always writes to `config.json`).
- Risk: A user upgrading from a pre-config.json release has their `api_hash` saved in `settings.json`. The new code reads from `settings.json` only if `config.json` lacks `api_id`. After the user's first auth save with the new code, `config.json` exists and `settings.json` becomes orphaned — never deleted, never overwritten. The `handleLogout`/`forceLogout` calls `store.delete('api_hash')` on whichever store was loaded *this session*, so a fresh-launch logout deletes from `config.json` only.
- Verification: real. Lines 87-91 (forceLogout) and 107-111 (handleLogout) only act on the active `store` reference.
- Fix: on app start, if `config.json` exists and `settings.json` exists too, migrate then delete `settings.json`. Add a one-time cleanup pass in `initStore`.

### App data dir read-recursive grants frontend access to bandwidth.json upload/download history

- **Severity:** LOW
- Files: `app/src-tauri/capabilities/default.json:11-12`, `bandwidth.rs:39` (writes `<app_data_dir>/bandwidth.json` with daily byte counts)
- Risk: Any frontend XSS can read the user's daily transfer history. Less serious than session theft but a privacy leak.
- Fix: same as CRITICAL #1 above — drop the recursive permissions.

---

## Scaling Limits

### 250 GB/day bandwidth cap is hardcoded

- **Severity:** LOW
- Files: `app/src-tauri/src/bandwidth.rs:51`
- Current: `limit: 250 * 1024 * 1024 * 1024`
- Limit: User can't configure it. Telegram's actual abuse-detection threshold is less generous for some account ages.
- Fix: expose via settings; persist to `bandwidth.json` alongside counters.

### Streaming server is single-port, no auth-rotation

- **Severity:** LOW
- Files: `app/src-tauri/src/lib.rs:65-76`
- Limit: Port 14200 is hardcoded. If another app on the user's machine is using 14200, app silently fails to start the server (only errors logged). Upload queue + download queue + streaming compete for one tokio executor and one MTProto sender pool.
- Fix: try-bind from 14200..14210; expose chosen port via `cmd_get_stream_port`.

### `cmd_search_global` has hardcoded `limit: 50`

- **Severity:** LOW
- Files: `app/src-tauri/src/commands/fs.rs:354`
- Limit: Search results capped at 50 with no "load more." Users with large drives won't find files past page 1.
- Fix: paginate using `offset_rate`/`offset_peer` (those fields exist in the request, lines 351-352, but are always set to 0/Empty).

---

## Dependencies at Risk

### `grammers-*` pinned to a git rev (not crates.io)

- **Severity:** MEDIUM
- Files: `app/src-tauri/Cargo.toml:23-26` — `git = "https://github.com/Lonami/grammers", rev = "d07f96f"`
- Risk: Upstream is unpublished on crates.io; if the GH repo is renamed, taken down, or rev rebased, builds break and we lose the ability to apply security fixes without manual cherry-pick. No `Cargo.lock` audit visible.
- Migration plan: vendor via `cargo vendor` and commit; or wait for the maintainer to publish.

### `actix-web 4` + `actix-cors 0.7` for a single endpoint

- **Severity:** LOW
- Files: `app/src-tauri/Cargo.toml:39-43`
- Risk: ~400 transitive deps for one streaming GET handler. Larger attack surface, longer build times. Tauri itself ships hyper/axum-compatible plumbing.
- Migration plan: rewrite `server.rs` on `axum` (already a tokio dep transitively) or `tiny_http` — eliminates ~30% of the dep graph.

### `pdfjs-dist 5.6.205` runs in a worker — verify CSP `worker-src 'self' blob:`

- **Severity:** LOW
- Files: `app/src-tauri/tauri.conf.json:32` (CSP allows `worker-src 'self' blob:`); pdfjs usage at `app/src/components/dashboard/PdfViewer.tsx`.
- Risk: pdfjs loads worker scripts at runtime; if pdfjs ships an exploit that escapes the worker (multiple historical CVEs in pdfjs), the WebView is compromised. With CRITICAL #1, that's session theft.
- Mitigation: keep pdfjs current; consider rendering PDFs server-side (Rust `pdfium`) for high-security mode.

---

## Missing Critical Features

### No `[TD]`-marker enforcement on the backend

- See CRITICAL #2 above. Architectural; all destructive paths need this.

### No real cancel for in-flight transfers

- See "Cancel-all uploads/downloads" under Known Bugs.

### No rate limiting / request-coalescing in the frontend

- Multiple `cmd_get_thumbnail` invocations during a fast scroll can swamp the runner. No de-dupe / batching.

### No telemetry for "failed to update" / "patched AppImage signature mismatch"

- Updater silently fails (CHANGELOG style suggests it's been working but #LOW above questions whether it does on the patched AppImage).

---

## Test Coverage Gaps

### Zero automated tests across the entire repo

- **Severity:** HIGH
- What's not tested: everything. CLAUDE.md states "No test suite exists. No lint script." Both Rust (`cargo test` will run zero tests) and TypeScript (no `vitest`/`jest`) have no tests.
- Files: project root has no `tests/` dir; `app/src-tauri/Cargo.toml` has no `[dev-dependencies]`; `app/package.json:6-11` has no `test` script.
- Risk: every change ships untested. Especially dangerous for the runner-lifecycle code (claim #7) where the bug it fixes is "stack overflow accumulates over hours of use" — manual QA can't catch that.
- Priority: HIGH for `auth.rs::ensure_client_initialized` (mock the SenderPool, assert old runner is signalled before new spawn), HIGH for `bandwidth.rs::check_and_reset` (date logic), MEDIUM for `resolve_peer` cache (once added).

### No integration test for the streaming server

- **Severity:** HIGH
- What's not tested: token validation, folder_id parsing, peer resolution failures, mid-stream client disconnect, simultaneous range requests.
- Files: `app/src-tauri/src/server.rs`
- Risk: regressions in token validation (CRITICAL #3) ship without warning.

### No e2e test for auth flow

- **Severity:** MEDIUM
- What's not tested: code request → 2FA → session save → reconnect.
- Risk: silent breakage of recovery paths (corrupt session, expired auth).

### No CI lint gate

- **Severity:** MEDIUM
- Files: `.github/workflows/main.yml` (need to check), `.github/workflows/release.yml` (build only, no clippy/tsc)
- Risk: clippy warnings re-accumulate.

---

## Notes on Verification of Seed Claims

| # | Claim | Verdict | Notes |
|---|---|---|---|
| 1 | fs:default + appdata recursive perms | **CONFIRMED** | `capabilities/default.json:11-14`. Worse: `api_hash` is also there, plaintext. |
| 2 | Destructive ops not [TD]-scoped | **CONFIRMED** | Verified in fs.rs `cmd_delete_folder` lines 74-108 and `cmd_delete_file` 168-184. |
| 3 | Streaming server allows arbitrary media exfiltration | **CONFIRMED** | server.rs:19-95 has no folder-id whitelist after token check. |
| 4 | CORS allows :1420 in release | **CONFIRMED** | server.rs:113. No `cfg!(debug_assertions)` gate. |
| 5 | `cmd_log` unauthenticated | **CONFIRMED but unused** | utils.rs:26-29; `grep` shows zero callers in `app/src/`. Vector exists, exposure low. |
| 6 | `cmd_clean_cache` skips thumbnails | **CONFIRMED** | preview.rs:161-174 only handles `previews/`; thumbnails dir at `<app_data_dir>/thumbnails/` never cleaned. |
| 7 | Grammers runner lifecycle critical | **CONFIRMED** | Verified in auth.rs:31-98. Fix is in place; the 100ms sleep is the only smell. |
| 8 | `cmd_is_network_available` hardcodes single DC IP | **CONFIRMED** | network.rs:14-17 — DC2 only. |
| 9 | AppImage post-patched after signing | **CONFIRMED, HIGH-IMPACT** | release.yml:99-108 signs during build, lines 110-250 patch after. Updater will reject the patched AppImage unless re-signed. Promoted to HIGH. |
| 10 | `settings.json` legacy api_hash | **CONFIRMED** | useTelegramConnection.ts:30 fallback; logout deletes only from active store. |
| 11 | `cmd_search_global` Messages/Slice copy-paste | **CONFIRMED** | fs.rs:361-413, two near-identical 26-line blocks. |
| 12 | `resolve_peer` O(N) per call | **CONFIRMED** | utils.rs:6-24, called from 8+ command paths and the streaming server. |

### Additional concerns discovered (not in seed)

- Path-traversal via Telegram-attribute filename (HIGH security).
- Cancel-all doesn't actually abort in-flight transfers (MEDIUM bug).
- `cmd_get_files` unbounded `iter_messages` (HIGH perf).
- Thumbnail downloads pull full-resolution media (MEDIUM perf).
- `cmd_search_global` `.unwrap()` on `d.document` panics on `DocumentEmpty` (MEDIUM bug).
- `app_data_dir().unwrap()` in `cmd_logout` (MEDIUM stability).
- Three-way version drift (1.1.2 vs 1.1.6 vs 1.1.6) in `package.json`/`Cargo.toml`/`tauri.conf.json` (MEDIUM debt).
- Streaming server `Content-Length: 0` for `Media::Photo` while streaming bytes (LOW protocol bug).
- Zero test suite across entire repo (HIGH; documented in CLAUDE.md).
- pdfjs-dist worker + recursive fs perms = compounded XSS exposure (LOW).
- Mock-mode reachable in release builds (MEDIUM debt).
- `useTelegramConnection` retry path can infinite-loop via `window.location.reload()` (MEDIUM bug).
- `cmd_create_folder` ignores `SetHistoryTtl` failure (LOW; user data loss vector).

---

*Concerns audit: 2026-04-29*
