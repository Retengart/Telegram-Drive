# Testing Patterns

**Analysis Date:** 2026-04-29

## TL;DR

**There is no automated test suite in this repository.** Zero unit tests, zero
integration tests, zero E2E tests, no test runner configured, no mocking
framework. Verification is done entirely through:

1. **Type checking** (`tsc` inside `npm run build`).
2. **Lint-as-compile** (`cargo clippy`).
3. **Manual QA** in `npm run tauri dev`.
4. **CI build** (`tauri-action` on every push to `main`).

Project root `CLAUDE.md` states this explicitly:

> No test suite exists. No lint script — TypeScript is checked via `tsc`
> inside `npm run build`; Rust via `cargo clippy`.

If you are adding new behavior, you are responsible for re-running all four
verification mechanisms by hand. There is no `npm test` to lean on.

---

## Confirmed Absence

The following common test artifacts are **not present** anywhere in the repo:

| Artifact                                       | Status        |
|------------------------------------------------|---------------|
| `**/*.test.ts`, `**/*.test.tsx`, `*.spec.*`    | None          |
| `app/src/**/__tests__/`                        | None          |
| `app/src-tauri/tests/`                         | None          |
| `#[cfg(test)] mod tests` blocks                | None          |
| `jest.config.*`, `vitest.config.*`             | None          |
| `playwright.config.*`, `cypress.config.*`      | None          |
| `@testing-library/*`, `vitest`, `jest` in `package.json` | None |
| `dev-dependencies` for testing in `Cargo.toml` | None          |
| `cargo test` or `npm test` step in CI          | None          |
| `cargo clippy` step in CI                      | None          |
| `tsc --noEmit` step in CI separate from build  | None          |

The `app/.npm-cache/` directory is build artifact, not test fixtures.

---

## What IS Run for Verification

### 1. TypeScript Strict Compile

`app/tsconfig.json` enables:

```json
"strict": true,
"noUnusedLocals": true,
"noUnusedParameters": true,
"noFallthroughCasesInSwitch": true,
"noEmit": true
```

`npm run build` is defined as `tsc && vite build` in `app/package.json:8`.
The `tsc` step runs first; **a type error fails the build**. There is no
separate `tsc --noEmit` step — the build itself is the type check.

**Run before every commit:**

```bash
cd app
npm run build
```

This catches:
- All type errors (strict mode).
- Unused locals / parameters (treated as errors).
- Fallthrough switch cases.

It does **not** catch:
- Logic bugs.
- Runtime errors.
- Stylistic issues (no Prettier, no ESLint).
- React anti-patterns (no `eslint-plugin-react-hooks`).

### 2. Rust Clippy

There is no `clippy.toml`, no `rustfmt.toml`, and no `[lints]` table in
`app/src-tauri/Cargo.toml`. Default clippy rules apply.

CHANGELOG entry 1.0.4 explicitly mentions the workflow:

> Ran Clippy and fixed all 7 warnings, including a couple of
> `collapsible_match` ones in `fs.rs` that needed manual refactoring.

**Run before every commit touching Rust:**

```bash
cd app/src-tauri
cargo clippy
# or for stricter:
cargo clippy -- -D warnings
```

`cargo check` is also useful as a faster type-only pass:

```bash
cd app/src-tauri
cargo check
```

Per `CLAUDE.md`, both are documented as the Rust-only verification commands.

### 3. Manual Smoke Test in Dev Mode

```bash
cd app
npm run tauri dev
```

This launches Vite at `:1420` and the Tauri WebView with hot-reload Rust.
**Manual paths to exercise** (derived from CHANGELOG hot-fix history,
file structure, and `CLAUDE.md`):

- **Auth wizard** (`AuthWizard.tsx`): API ID/Hash entry → phone → code →
  optional 2FA password. Verify FLOOD_WAIT countdown still parses (it
  depends on the `FLOOD_WAIT_<seconds>` shape from `map_error` in
  `commands/utils.rs:36`).
- **Dev-mode bypass**: in dev, the auth screen exposes a "Dev Mode" button
  (`AuthWizard.tsx:289-297`) that calls `onLogin()` without contacting
  Telegram. This puts the app into the **mock-mode** code paths (every
  `cmd_*` returns mock data when `TelegramState.client` is `None`) — useful
  for iterating on UI without burning auth attempts.
- **Folder operations**: create / delete / sync. Sync must find both
  `[TD]` title-marked and `[telegram-drive-folder]` about-marked channels.
- **File ops**: upload (drag + picker), download, move (drag-drop), delete,
  bulk-select, bulk-download, bulk-delete, bulk-move.
- **Preview**: image preview (base64 data URL), PDF (`PdfViewer.tsx`),
  video / audio streaming via `localhost:14200` (`MediaPlayer.tsx`). The
  stream URL requires the per-launch token from `cmd_get_stream_token`.
- **Search**: type >2 chars → triggers `cmd_search_global` after 500ms
  debounce (`Dashboard.tsx:168-182`).
- **Theme toggle**: light / dark, persisted to `localStorage`.
- **Update banner**: appears 5s after login if a newer release tag exists.
- **Window resize**: grid columns recompute (2 / 3 / 4 / 5 / 6 columns),
  virtualizer must not produce overlapping rows. **Critical regression
  area** — CHANGELOG 1.0.3 + 1.0.4 are both fixes for this exact bug. The
  current solution uses pixel heights, not CSS aspect ratios. If you touch
  `FileExplorer.tsx` or `FileCard.tsx`, resize the window through every
  breakpoint and confirm no overlap.
- **Network drop**: pull the network cable / disable wifi while the app is
  open. The `useNetworkStatus` hook polls `cmd_is_network_available` every
  10s; the connection dot in the sidebar should turn red.
- **Ctrl+C from terminal**: launch via `npm run tauri dev`, then Ctrl+C in
  the terminal. Process MUST exit cleanly (not hang). This is the regression
  fixed in v1.1.6 — the `RunEvent::Exit` handler in `lib.rs:107-129` must
  shut down both the grammers runner and the Actix server.
- **Linux AppImage**: can only be tested by building a release tag and
  pulling the AppImage from the GitHub release. The post-build patching
  step in CI strips bundled Mesa libs; CHANGELOG 1.1.3 / 1.1.4 / 1.1.5
  document the EGL_BAD_ALLOC fix on Arch / rolling distros.

### 4. CI Build

`.github/workflows/main.yml` runs on every push to `main` and uses
`tauri-apps/tauri-action@v0` to do a full release build on Windows,
Ubuntu 22.04, and macOS-arm64 in parallel. **It does not run tests, clippy,
or a separate `tsc` step** — it just runs `npm install` then the `tauri
build`, which transitively runs `tsc && vite build` (so type errors do
fail CI). Linker errors, missing native deps, etc. would also surface
here.

`.github/workflows/release.yml` runs on `v*` tags and additionally
post-processes the Linux AppImage (strip Mesa, replace AppRun). It
similarly does no testing.

---

## Why No Tests?

This is a thin Tauri shell over the `grammers` Telegram client. The bulk of
"interesting" logic lives behind:

1. **Network I/O** to Telegram (hard to mock without an integration harness).
2. **Filesystem** (sessions, caches, bandwidth journal).
3. **Rendering** (WebKitGTK / WebView2 / WKWebView WebView).
4. **GPU / OS-specific bugs** (the EGL issue could only ever be reproduced
   live on Arch).

The mock-mode paths (`if client_opt.is_none() { return Ok(mock); }`
scattered through `commands/fs.rs`, `commands/preview.rs`, etc.) act as a
crude stand-in for unit tests — they let the frontend run end-to-end
without a real Telegram session.

---

## If You Add Tests

There is no precedent to follow. Reasonable starting points:

### Rust (suggested only)

Inline `#[cfg(test)] mod tests {…}` blocks at the bottom of each
`commands/*.rs` file would be the path of least resistance. The pure
helpers are the lowest-hanging fruit:

- `app/src-tauri/src/commands/utils.rs:36-52` — `map_error()` for
  `FLOOD_WAIT_<n>` parsing has clear input/output and zero side effects.
- `app/src-tauri/src/bandwidth.rs` — `format_bytes()`, `check_and_reset()`,
  `can_transfer()` are mostly pure (modulo filesystem write in `save_locked`).

Run with: `cd app/src-tauri && cargo test`.

### TypeScript (suggested only)

`app/src/utils.ts` is pure (`formatBytes`, `isVideoFile`, `isImageFile`,
etc.) and would be a clean Vitest target. There is currently no Vitest
config — adding one means: `npm i -D vitest`, a `vitest.config.ts`, a
`test` script in `package.json`, and a CI step.

For component / hook tests you would need `@testing-library/react` and
`jsdom` — none currently installed.

### CI

If you add tests, also add a CI step. `tauri-action` does not run them
automatically. A minimal addition to `.github/workflows/main.yml` would be:

```yaml
- name: cargo test
  working-directory: app/src-tauri
  run: cargo test

- name: npm test
  working-directory: app
  run: npm test
```

Currently neither command is wired up in CI.

---

## Coverage

Not measured. Not enforced. There is no coverage tool installed.

---

## Mocking

There is no mocking framework. The only "mocks" are the in-source mock-mode
branches in Tauri commands (`commands/fs.rs:20-28`, `commands/fs.rs:84-87`,
`commands/preview.rs:60-63`, etc.) which return canned data when the
Telegram client is uninitialized. These exist for **dev iteration**, not
for isolation testing.

---

## Test Data / Fixtures

None. The single `app/test_upload.txt` file looks like a leftover scratch
file from manual upload testing — it's a 136-byte text file, not a fixture
in any structured sense.

---

## Common Patterns (none)

There is nothing to document under sub-sections like "Async Testing",
"Error Testing", "Suite Organization" etc. — adding them would be inventing
patterns that don't yet exist in this codebase.

---

## Verification Checklist

Before opening a PR, run all four locally:

```bash
# 1. Frontend type check + build
cd app
npm run build

# 2. Rust lint
cd src-tauri
cargo clippy
cd ..

# 3. Manual smoke
npm run tauri dev
# (exercise the changed code path; if you touched grid/virtualizer code,
#  resize the window through every breakpoint)

# 4. Confirm clean Ctrl+C
# (start the dev command in a terminal, then Ctrl+C — process must exit)
```

Until tests exist, this is the contract.

---

*Testing analysis: 2026-04-29*
