//! Telegram peer scoping for SCOPE-01..04.
//!
//! Every destructive `#[tauri::command]` and the streaming entry point calls
//! [`require_td_peer`] before issuing a Telegram operation. The helper
//! enforces that the target peer is a Telegram-Drive-marked channel (title
//! contains `[TD]` case-insensitively, OR about contains
//! `[telegram-drive-folder]`), or — where allowed — the user's own peer
//! (Saved Messages).
//!
//! Cache: `TelegramState.td_channel_cache` (HashSet<i64>) is the fast path.
//! Populated by `cmd_scan_folders` (wholesale replace) and `cmd_create_folder`
//! (insert), invalidated by `cmd_delete_folder` (remove) and `cmd_logout`
//! (clear).
//!
//! Cache-HIT trade-off (D-02, WR-02): a cache hit returns the resolved peer
//! WITHOUT re-checking the live `[TD]` title or `[telegram-drive-folder]`
//! about marker. If the user manually unmarks a channel via the Telegram
//! desktop client between scans, destructive ops on that channel still pass
//! the gate until the next `cmd_scan_folders` or `cmd_logout` rebuilds the
//! cache. Auto-correction only fires on cache MISS. Threat is low (the user
//! is the one who removed the marker), but cache HIT is the silent-failure
//! path — keep this in mind when reviewing future cache-policy changes.

use grammers_client::Client;
use grammers_client::types::Peer;
use grammers_tl_types as tl;
use crate::commands::TelegramState;
use crate::commands::utils::{resolve_peer, map_error};

/// Typed error returned by [`require_td_peer`]. Implements `serde::Serialize`
/// so call sites can JSON-encode it for the IPC `Result<T, String>` contract
/// (see Pattern 1 in PATTERNS.md).
#[derive(Debug, Clone, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopingError {
    #[error("peer for folder_id={folder_id} is not a Telegram Drive folder")]
    NotTdPeer { folder_id: i64 },

    #[error("cannot perform this operation on Saved Messages")]
    CannotDeleteSavedMessages,

    #[error("peer resolution failed for folder_id={folder_id:?}: {reason}")]
    PeerResolutionFailed { folder_id: Option<i64>, reason: String },

    #[error("peer for folder_id={folder_id} resolved to non-channel")]
    NotAChannel { folder_id: i64 },

    #[error("Telegram client is not connected")]
    ClientNotConnected,
}

/// Pure marker check used by both the cache-miss live path and (eventually)
/// `cmd_scan_folders`. Title check is case-insensitive (matches existing
/// `cmd_scan_folders` behavior at fs.rs:443); about marker is the canonical
/// `[telegram-drive-folder]` literal written by `cmd_create_folder`.
pub fn peer_is_marked(title: &str, about: Option<&str>) -> bool {
    if title.to_lowercase().contains("[td]") {
        return true;
    }
    if let Some(a) = about {
        if a.contains("[telegram-drive-folder]") {
            return true;
        }
    }
    false
}

/// Gate every destructive Telegram operation. Returns the resolved `Peer`
/// so the caller does not pay a second `resolve_peer` round-trip.
///
/// `allow_saved_messages: false` rejects `folder_id == None` with
/// [`ScopingError::CannotDeleteSavedMessages`]. `true` permits it and
/// returns the user's own peer (`Peer::User`).
pub async fn require_td_peer(
    state: &TelegramState,
    client: &Client,
    folder_id: Option<i64>,
    allow_saved_messages: bool,
) -> Result<Peer, ScopingError> {
    // Saved Messages branch — top of helper, before any I/O.
    let fid = match folder_id {
        None => {
            if !allow_saved_messages {
                return Err(ScopingError::CannotDeleteSavedMessages);
            }
            // `resolve_peer(client, None)` returns Peer::User(me). Map any
            // failure (e.g., FLOOD_WAIT during get_me) into our typed error.
            return resolve_peer(client, None)
                .await
                .map_err(|reason| ScopingError::PeerResolutionFailed {
                    folder_id: None,
                    reason,
                });
        }
        Some(id) => id,
    };

    // Cache fast path. Drop the read guard before any further await.
    let in_cache = {
        let guard = state.td_channel_cache.read().await;
        guard.contains(&fid)
    };

    // Resolve peer (always — the cache only short-circuits the marker check,
    // we still need the InputPeer/access_hash for the caller's downstream op).
    let peer = resolve_peer(client, Some(fid))
        .await
        .map_err(|reason| ScopingError::PeerResolutionFailed {
            folder_id: Some(fid),
            reason,
        })?;

    // Reject DM / Chat peers — Telegram Drive folders are channels.
    let channel_ref = match &peer {
        Peer::Channel(c) => c,
        _ => return Err(ScopingError::NotAChannel { folder_id: fid }),
    };

    if in_cache {
        return Ok(peer);
    }

    // Cache miss: live `channels::GetFullChannel` check on the about field.
    // Pattern copied verbatim from cmd_scan_folders at fs.rs:451-468.
    let access_hash = channel_ref
        .raw
        .access_hash
        .ok_or_else(|| ScopingError::PeerResolutionFailed {
            folder_id: Some(fid),
            reason: "channel has no access_hash".to_string(),
        })?;
    let input_chan = tl::enums::InputChannel::Channel(tl::types::InputChannel {
        channel_id: channel_ref.raw.id,
        access_hash,
    });

    let title = channel_ref.raw.title.clone();

    let is_marked = match client
        .invoke(&tl::functions::channels::GetFullChannel { channel: input_chan })
        .await
    {
        Ok(tl::enums::messages::ChatFull::Full(f)) => match f.full_chat {
            tl::enums::ChatFull::Full(cf) => peer_is_marked(&title, Some(&cf.about)),
            _ => peer_is_marked(&title, None),
        },
        Err(e) => {
            // Preserve FLOOD_WAIT shape via map_error so AuthWizard.tsx:135-144
            // can still parse the error code.
            return Err(ScopingError::PeerResolutionFailed {
                folder_id: Some(fid),
                reason: map_error(e),
            });
        }
    };

    if !is_marked {
        return Err(ScopingError::NotTdPeer { folder_id: fid });
    }

    // Cache the positive result. Short write guard, no await held.
    {
        let mut guard = state.td_channel_cache.write().await;
        guard.insert(fid);
    }

    Ok(peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_is_marked_title_uppercase() {
        assert!(peer_is_marked("My Folder [TD]", None));
    }

    #[test]
    fn peer_is_marked_title_lowercase() {
        assert!(peer_is_marked("my folder [td]", None));
    }

    #[test]
    fn peer_is_marked_title_mixed_case() {
        assert!(peer_is_marked("My Folder [Td]", None));
        assert!(peer_is_marked("My Folder [tD]", None));
    }

    #[test]
    fn peer_is_marked_about_marker() {
        assert!(peer_is_marked(
            "Random Channel",
            Some("Telegram Drive Storage Folder\n[telegram-drive-folder]")
        ));
    }

    #[test]
    fn peer_is_marked_neither_marker() {
        assert!(!peer_is_marked("Random Channel", Some("just a channel")));
        assert!(!peer_is_marked("Random Channel", None));
    }

    #[test]
    fn peer_is_marked_both_markers() {
        assert!(peer_is_marked("[TD]", Some("[telegram-drive-folder]")));
    }

    #[test]
    fn peer_is_marked_empty_inputs() {
        assert!(!peer_is_marked("", None));
        assert!(!peer_is_marked("", Some("")));
        assert!(!peer_is_marked("", Some("not marked")));
    }

    #[test]
    fn peer_is_marked_does_not_match_partial_substring() {
        // A channel titled "studio" should NOT match — substring "td" is not "[td]".
        assert!(!peer_is_marked("studio", None));
        // "[TDR]" should NOT match (different bracket content).
        assert!(!peer_is_marked("Channel [TDR]", None));
    }

    #[test]
    fn scoping_error_serializes_cleanly() {
        let e = ScopingError::NotTdPeer { folder_id: 12345 };
        let json = serde_json::to_string(&e).expect("ScopingError is serializable");
        // Externally-tagged via `#[serde(tag = "kind", rename_all = "snake_case")]`.
        assert!(json.contains("\"kind\":\"not_td_peer\""), "json was: {}", json);
        assert!(json.contains("\"folder_id\":12345"), "json was: {}", json);
    }

    #[test]
    fn scoping_error_saved_messages_serializes() {
        let e = ScopingError::CannotDeleteSavedMessages;
        let json = serde_json::to_string(&e).expect("serializable");
        assert!(json.contains("\"kind\":\"cannot_delete_saved_messages\""), "json was: {}", json);
    }
}
