# Telegram Drive

## What This Is

Cross-platform desktop app (Tauri 2 + React 19 + Rust) that turns the user's own Telegram account into personal cloud storage. Files live in private Telegram channels marked `[TD]`; the user gets a Google-Drive-like UI with upload/download, search, video/PDF streaming, and bandwidth metering — all driven by an in-process MTProto client (`grammers`).

## Core Value

The user's Telegram session must never leak. Account takeover is the unrecoverable failure mode — every other bug is graceful.

## Requirements

### Validated

<!-- Inferred from existing codebase + ARCHITECTURE.md / STACK.md / CONCERNS.md -->

- ✓ Telegram MTProto auth with phone+code, optional 2FA — existing
- ✓ Persistent SQLite session at `<app_data_dir>/telegram.session` — existing
- ✓ Folders modelled as private TG channels with `[TD]` title suffix or `[telegram-drive-folder]` about marker — existing
- ✓ `cmd_create_folder` / `cmd_delete_folder` / `cmd_move_files` / file CRUD — existing
- ✓ React Query–backed file listing with virtualized grid/list views — existing
- ✓ In-process Actix-web streaming server on `127.0.0.1:14200` for video/audio/PDF — existing (will be replaced this milestone)
- ✓ Per-launch random 32-char stream token — existing (will be replaced this milestone)
- ✓ Bandwidth gate (`BandwidthManager`, 250 GB/day, midnight reset) — existing
- ✓ Preview cache (LRU 30 files / 80 MB) + thumbnail cache — existing
- ✓ Global cross-chat search via `cmd_search_global` — existing
- ✓ Tauri 2 auto-updater with Ed25519-signed `latest.json` — existing
- ✓ Linux EGL/GL workaround: `WEBKIT_DISABLE_DMABUF_RENDERER=1` + AppImage `AppRun` wrapper — existing
- ✓ Graceful Ctrl+C shutdown of grammers runner + Actix server via `RunEvent::Exit` — existing (v1.1.6)
- ✓ Mock-mode fallback when `TelegramState.client` is `None` — existing

### Active

<!-- This milestone: security hardening — close all 12 audit findings before next public release. -->

**CRITICAL**

- [ ] Strip `fs:*-appdata-recursive` from frontend capabilities; route all FS access through scoped IPC commands so the WebView cannot read `telegram.session` or `config.json` (audit #1)
- [ ] Enforce `[TD]`-folder scoping in destructive backend commands (`cmd_delete_folder`, `cmd_delete_file`, `cmd_move_files`) — reject any peer without the marker (audit #2)
- [ ] Replace Actix streaming server with Tauri custom URI scheme (`register_asynchronous_uri_scheme_protocol`) so streams are IPC-bound, no HTTP, no token in URL, no CORS surface (audit #3, #10, #12)

**MEDIUM**

- [ ] Move `api_id` / `api_hash` from plaintext `config.json` to OS keychain via `keyring` crate (audit #4)
- [ ] On logout, wipe `<app_data_dir>/thumbnails/` in addition to previews (audit #5)
- [ ] Lock down `cmd_log`: drop `\n`/`\r`, cap length, rate-limit per-process — or remove if unused (audit #6)
- [ ] Purge legacy `settings.json` after one-time migration to `config.json` so stale `api_hash` doesn't survive upgrades (audit #7)
- [ ] Gate `cmd_logout` against unintended invocation (frontend confirmation token + backend check) so an XSS cannot silently sign the user out (audit #8)

**LOW**

- [ ] Fix AppImage release pipeline: re-upload patched AppImage AND regenerate the Ed25519 `latest.json` signature so the auto-updater verifies (audit #9)
- [ ] Replace hardcoded TG IP `149.154.167.50` reachability probe with a robust check (audit #11)
- [ ] Remove `Cache-Control: private, max-age=120` from streamed media (subsumed by URI-scheme refactor in audit #3) (audit #12)

**MIGRATION**

- [ ] Best-effort migrate existing v1.1.x users: read legacy `config.json` / `settings.json`, transparently relocate secrets to keychain, drop legacy stores; on failure fall back to re-login

### Out of Scope

- Existing tech debt unrelated to security hardening (mock-mode coupling, three-way version drift, `O(N dialogs)` `resolve_peer`, `cmd_get_files` unbounded walk) — tracked in `codebase/CONCERNS.md`, defer to subsequent milestones
- UX redesign or new features — milestone is hardening only; no surface-area additions
- Encrypting the SQLite session file at rest (separate larger effort) — defer; keychain-wrapping the session DB is its own milestone
- A user-facing migration wizard — best-effort silent migration is enough; we accept forced re-login as fallback
- Refactoring `cmd_search_global` to scope by `[TD]` channels only — out-of-scope (UI feature, not a security bug)

## Context

- Tauri 2 desktop app, two async runtimes inside the Rust binary: tokio (IPC handlers) and actix-rt (streaming server). Streaming server will be removed this milestone in favour of Tauri's URI scheme protocol.
- Frontend stack: React 19, framer-motion, lucide-react, sonner, pdfjs-dist, @tanstack/react-query, react-virtual. Each is a potential XSS / supply-chain entry point — we MUST assume the frontend is hostile and harden the backend boundary.
- `grammers` is pinned to git rev `d07f96f`. We do not change the MTProto client this milestone.
- Codebase already mapped: `.planning/codebase/{ARCHITECTURE,STACK,CONCERNS,STRUCTURE,CONVENTIONS,INTEGRATIONS,TESTING}.md`. Audit findings overlap heavily with `CONCERNS.md` Security Considerations section.
- Releases: GitHub Actions on `v*` tag → `tauri-action@v0` builds & uploads → AppImage post-build patch step (currently doesn't re-upload OR re-sign — confirmed bug for audit #9).
- Updater pubkey + endpoint pinned in `tauri.conf.json`. `TAURI_SIGNING_PRIVATE_KEY{,_PASSWORD}` from GH secrets.
- Current version: `app/package.json = 1.1.2`, `Cargo.toml = 1.1.6`, `tauri.conf.json` — three-way drift (already documented as tech debt). Bump-on-release will reconcile.

## Constraints

- **Security**: Threat model assumes the WebView is hostile (XSS / dep compromise). Backend must treat every IPC arg and HTTP query as untrusted. Personal-account takeover = unrecoverable → all CRITICAL items are release blockers.
- **Tech stack**: Tauri 2, Rust, `grammers` pinned. No swap of MTProto client. `keyring` crate must support Linux secret-service, macOS Keychain, Windows Credential Manager.
- **Compatibility**: Best-effort migration of existing v1.1.x sessions. Force re-login is acceptable fallback when migration fails. No support for downgrade (we may delete legacy stores after migration).
- **Granularity**: Coarse phasing (3–5 phases). Mode = YOLO, parallel plans, planning docs local-only (`.planning/` in `.gitignore`). Per-phase research + plan-check + verifier all enabled.
- **Release**: Single bundled milestone — all 12 issues ship together (next minor version). No partial releases.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Streaming server → Tauri custom URI scheme (drop Actix) | Eliminates root cause of audit #3, #10, #12 in one move; IPC-bound = no token-in-URL, no CORS surface, no HTTP cache leak. Cleaner Tauri 2 idiom. | — Pending |
| Secrets → OS keychain via `keyring` crate | Cross-platform standard; defence-in-depth even after audit #1 closes | — Pending |
| Migration: best-effort silent | Don't force users through full re-auth on upgrade; fall back to re-login when migration fails | — Pending |
| All 12 issues in one milestone | Coherent security release; LOW items piggyback on CRITICAL refactors | — Pending |
| Skip project-level domain research (4-agent) | `codebase/CONCERNS.md` already covers the security domain analysis with file:line refs; redundant. Per-phase research stays enabled. | — Pending |
| Coarse granularity | Each phase ≈ a coherent attack-surface domain; matches the audit's groupings | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-29 after initialization (security hardening milestone)*
