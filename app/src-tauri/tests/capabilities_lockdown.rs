//! Regression test for CAP-03: fails the build if banned FS permissions re-appear
//! in `capabilities/default.json`. Establishes the test-layout convention for
//! Phase 2/3/4 per CONTEXT D-11.

use std::path::PathBuf;

/// Permissions that must NEVER appear (exact match).
const FORBIDDEN_EXACT: &[&str] = &["fs:default"];

/// Permission substrings that must NEVER appear when prefixed with `fs:`.
/// Per CONTEXT D-04, ban any `recursive`, `write`, or `meta` fs permission —
/// including non-recursive variants such as `fs:allow-appdata-write` and
/// `fs:allow-appdata-meta`. The blast-radius is the original CRITICAL #1
/// (WebView reads `telegram.session`); narrowing this list re-opens it.
const FORBIDDEN_FS_SUBSTR: &[&str] = &["recursive", "write", "meta"];

#[test]
fn capabilities_default_does_not_grant_broad_fs_access() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("capabilities").join("default.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    let v: serde_json::Value = serde_json::from_str(&raw)
        .expect("capabilities/default.json is not valid JSON");

    let perms = v
        .get("permissions")
        .and_then(|p| p.as_array())
        .expect("capabilities/default.json missing `permissions` array");

    for perm in perms {
        let s = perm.as_str().unwrap_or("");

        // Rule 1: exact-match deny
        for banned in FORBIDDEN_EXACT {
            assert!(
                s != *banned,
                "CAP-03 regression: forbidden permission `{}` is back in capabilities/default.json",
                banned
            );
        }

        // Rule 2: substring deny — but ONLY for fs:* permissions
        if s.starts_with("fs:") {
            for banned in FORBIDDEN_FS_SUBSTR {
                assert!(
                    !s.contains(banned),
                    "CAP-03 regression: fs permission `{}` matches forbidden substring `{}`",
                    s, banned
                );
            }
        }
    }
}

#[test]
fn capabilities_test_rejects_synthetic_violation() {
    // W2: prove the banned-list logic actually rejects a synthetic violation
    // without needing adversarial reasoning. This test does NOT touch the on-disk
    // default.json — it builds an in-memory permissions array containing banned
    // entries and runs the same rule predicate against it, asserting EVERY
    // banned permission is detected (exact-match + substring + non-recursive).
    let synthetic = serde_json::json!({
        "identifier": "default",
        "windows": ["main"],
        "permissions": [
            "fs:default",                       // exact-match path
            "fs:allow-appdata-write-recursive", // substring path (recursive)
            "fs:allow-appdata-write",           // non-recursive write
            "fs:allow-appdata-meta",            // non-recursive meta
            "core:default",                     // benign
        ]
    });
    let perms = synthetic["permissions"].as_array().unwrap();
    let found_banned: Vec<&str> = perms
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|p| {
            FORBIDDEN_EXACT.contains(p)
                || (p.starts_with("fs:")
                    && FORBIDDEN_FS_SUBSTR.iter().any(|s| p.contains(s)))
        })
        .collect();
    // All FOUR synthetic banned perms must be caught (one exact + three substring).
    assert_eq!(
        found_banned.len(),
        4,
        "expected 4 banned perms, found {:?}",
        found_banned
    );
}
