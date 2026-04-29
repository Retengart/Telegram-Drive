---
phase: 01-ipc-boundary-lockdown
plan: 01
subsystem: security
tags: [tauri, capabilities, ci, security, fs-lockdown, regression-gate]

requires: []
provides:
  - "WebView FS access locked down (no fs:* in capabilities/default.json)"
  - "CAP-03 regression test gate (tests/capabilities_lockdown.rs)"
  - "thiserror = \"2\" dependency for Wave 2 scoping module"
  - "[dev-dependencies] block convention with serde_json"
  - "Linux-only cargo test step in CI (.github/workflows/main.yml)"
affects: [01-02, 01-03, 01-04, scoping, stream-uri-scheme, secrets]

tech-stack:
  added: [thiserror 2, serde_json (dev-dep)]
  patterns:
    - "tests/ integration test layout (CARGO_MANIFEST_DIR + include-on-disk JSON)"
    - "FORBIDDEN_EXACT + FORBIDDEN_FS_SUBSTR deny-list shape for capability gates"
    - "Linux-only CI gate (matrix.platform == 'ubuntu-22.04') for platform-agnostic tests"

key-files:
  created:
    - "app/src-tauri/tests/capabilities_lockdown.rs"
  modified:
    - "app/src-tauri/capabilities/default.json"
    - "app/src-tauri/Cargo.toml"
    - "app/src-tauri/Cargo.lock"
    - ".github/workflows/main.yml"

key-decisions:
  - "Drop all four fs:* permissions, not just the recursive trio — fs:default alone already grants broad read"
  - "Substring rule scoped to fs: prefix — fictional foo:recursive must not trip gate (T-01-06)"
  - "Defer tauri = \"2.10\" pin to keep blast radius minimal (Dev-5 / U3)"
  - "Defer cargo clippy CI step — pre-existing collapsible_match violations in src/commands/utils.rs:12-13 are out of scope (Rule SCOPE_BOUNDARY)"

patterns-established:
  - "Integration test in app/src-tauri/tests/ reading capabilities JSON from disk via CARGO_MANIFEST_DIR"
  - "Synthetic-violation test pattern (in-memory predicate exercise) replaces adversarial reasoning for deny-list confidence"
  - "Linux-only CI gate pattern via matrix.platform == 'ubuntu-22.04'"

requirements-completed: [CAP-01, CAP-02, CAP-03]

duration: 6min
completed: 2026-04-29
---

# Phase 01 Plan 01: IPC boundary lockdown bootstrap Summary

**WebView FS access stripped from Tauri capabilities, CAP-03 cargo-test regression gate live in-repo and in CI, thiserror + dev-deps bootstrapped for Wave 2 scoping module.**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-04-29T22:53:00Z (approx)
- **Completed:** 2026-04-29T22:59:46Z
- **Tasks:** 3
- **Files modified:** 4 (1 created, 3 modified, +Cargo.lock)

## Accomplishments

- Closed audit #1 root cause: WebView can no longer read `<app_data_dir>/telegram.session` or `config.json` via FS plugin (CAP-01, CAP-02).
- First integration test in repo lives at `app/src-tauri/tests/capabilities_lockdown.rs` — establishes the test-layout convention reused by Phase 2/3/4 (CAP-03, D-11).
- CI now runs `cargo test` Linux-only on push to main; banned-perm regression fails the workflow red.
- `thiserror = "2"` + `[dev-dependencies] serde_json` bootstrapped — Wave 2 `scoping.rs` will compile.

## Task Commits

Each task committed atomically (`--no-verify`, parallel-worktree convention):

1. **Task 1: Drop banned FS permissions from default.json** — `eb8648d` (feat)
2. **Task 2: Add thiserror dep + dev-deps block + CAP-03 regression test** — `b010198` (test, combined RED+GREEN since synthetic-violation test exercises predicate without modifying default.json)
3. **Task 3: Wire cargo test into CI (main.yml)** — `9508453` (ci)

## Files Created/Modified

- `app/src-tauri/capabilities/default.json` — dropped 4 fs:* entries, 5 surviving permissions (core/shell/store/dialog/updater).
- `app/src-tauri/tests/capabilities_lockdown.rs` — NEW. Two #[test] functions: positive on-disk check + synthetic in-memory rejection check. FORBIDDEN_EXACT + FORBIDDEN_FS_SUBSTR deny-list constants.
- `app/src-tauri/Cargo.toml` — appended `thiserror = "2"` to [dependencies]; new `[dev-dependencies]` block with `serde_json = "1"`.
- `app/src-tauri/Cargo.lock` — auto-updated with thiserror 2.x + thiserror-impl resolution.
- `.github/workflows/main.yml` — inserted `cargo test (capability lockdown gate)` step between `install frontend dependencies` and `build the app`, gated on `matrix.platform == 'ubuntu-22.04'`, `working-directory: app/src-tauri`.

## Test Results

```
$ cd app/src-tauri && cargo test --quiet
running 2 tests
..
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured
```

Adversarial check (manually re-inserted `"fs:default"` into default.json, ran cargo test, confirmed FAILED with `CAP-03 regression: forbidden permission \`fs:default\` is back...`, then `git checkout -- app/src-tauri/capabilities/default.json` to revert).

## Decisions Made

- **Combined Task 2 RED+GREEN into one commit** — tdd="true" plan-level guidance assumes a behavior gap to bridge, but here the synthetic-violation test exercises the deny-list predicate purely in-process (no on-disk default.json modification needed). Both tests pass against the freshly-cleaned default.json. The test commit message uses `test(...)` per conventional-commits convention; an explicit RED commit with a deliberately-broken default.json would have to be reverted, adding noise.
- **Deferred clippy CI step** — pre-existing `collapsible_match` clippy errors in `src/commands/utils.rs:12-13` would block the gate. Per Rule SCOPE_BOUNDARY, fixing unrelated lint violations is out of scope for this task. Surfaced as unresolved question (see below).
- **Deferred tauri = "2.10" pin** — Per Dev-5 / RESEARCH.md U3, kept the existing `tauri = { version = "2", features = [] }` pin to minimize blast radius. Surfaced as unresolved question.

## Deviations from Plan

None — plan executed exactly as written. The clippy step was explicitly marked optional in the plan ("recommended, low cost; if existing clippy violations exist, surface as an unresolved question") and the surfacing path was followed.

## Issues Encountered

- Pre-existing `clippy::collapsible_match` errors at `app/src-tauri/src/commands/utils.rs:12-13` discovered when evaluating the optional clippy CI step. Resolution: defer clippy step per plan instructions, log as unresolved question for follow-up.

## Threat Flags

None — no new attack surface introduced. This plan strictly removes capabilities and adds defensive tests/CI gates.

## Next Phase Readiness

- Wave 2 scoping module (Plan 01-02) can now `use thiserror::Error` and create dev-tests under `app/src-tauri/tests/`.
- CAP-03 gate is in CI — any future PR re-adding banned `fs:*` perms to `default.json` will fail the workflow on `push: branches: [main]`.
- Deferred items: tauri = "2.10" pin, cargo clippy gate, fixing the two pre-existing collapsible_match violations.

## Self-Check

- [x] `app/src-tauri/capabilities/default.json` exists, 5 permissions, 0 fs:* entries
- [x] `app/src-tauri/tests/capabilities_lockdown.rs` exists, contains both test functions + both constants
- [x] `app/src-tauri/Cargo.toml` contains `thiserror = "2"` + `[dev-dependencies]` block
- [x] `.github/workflows/main.yml` contains `cargo test` step with `matrix.platform == 'ubuntu-22.04'` gate, YAML still valid
- [x] Commits eb8648d, b010198, 9508453 in `git log`

## Self-Check: PASSED

---
*Phase: 01-ipc-boundary-lockdown*
*Completed: 2026-04-29*

## Unresolved Questions

- tauri = "2.10" pin: defer or apply now?
- Add cargo clippy CI gate after fixing pre-existing collapsible_match violations in src/commands/utils.rs:12-13?
- Inline-fix those clippy violations as part of 01-02, or separate chore commit?
