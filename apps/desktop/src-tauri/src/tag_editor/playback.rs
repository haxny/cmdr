//! Playing a session's tracks in the app, and opening them in a player.
//!
//! Playback goes through the viewer's `cmdr-media://` scheme (`file_viewer/media.rs`): a token is
//! minted only for a readable track of the tag session (compared by canonical path), never for a
//! path the frontend made up. This module keeps the tokens it minted per session and drops them on
//! release and when the tag session closes (`session::close`), so no token outlives its sheet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use cmdr_tags::format_of;
use tauri::{AppHandle, Runtime};

use super::session::{TagEditorError, is_open, loaded_tracks};
use crate::file_viewer::content_kind::{ViewerContentKind, audio_mime};
use crate::file_viewer::media::{MediaEntry, drop_token, mint_token};
use crate::file_viewer::media_protocol::SCHEME;
use crate::ignore_poison::IgnorePoison;

/// foobar2000 for Mac (verified on macOS 27.0.1 with foobar2000 installed, `mdls -name
/// kMDItemCFBundleIdentifier`, 2026-10-27).
#[cfg_attr(
    not(all(target_os = "macos", not(feature = "playwright-e2e"))),
    allow(dead_code, reason = "only the macOS app build looks foobar2000 up")
)]
pub const FOOBAR2000_BUNDLE_ID: &str = "com.foobar2000.mac";

const LOG_TARGET: &str = "tag_editor::playback";

/// The audio tokens minted per tag session.
static TOKENS: LazyLock<Mutex<HashMap<String, Vec<String>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A `cmdr-media://` URL that plays the track at `path` of the session. A path that isn't a
/// readable track of the session is `UnknownTrack`. Blocking (canonicalizes paths).
pub fn audio_url(session_id: &str, path: &str) -> Result<String, TagEditorError> {
    let (track, canonical) = match_tracks(session_id, std::slice::from_ref(&path.to_string()))?
        .pop()
        .ok_or(TagEditorError::UnknownTrack)?;
    let format = track
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(format_of)
        .ok_or(TagEditorError::UnknownTrack)?;
    let token = mint_token(MediaEntry {
        canonical_path: canonical,
        kind: ViewerContentKind::Audio,
        mime: audio_mime(format).to_string(),
    });
    TOKENS
        .lock_ignore_poison()
        .entry(session_id.to_string())
        .or_default()
        .push(token.clone());
    // The session may have closed while we minted, after its close already released: ask again.
    if !is_open(session_id) {
        release_session(session_id);
        return Err(TagEditorError::NotOpen);
    }
    Ok(format!("{SCHEME}://localhost/{token}"))
}

/// Drops the token of a URL `audio_url` gave. Any other URL (a viewer's token too) is ignored.
pub fn release(url: &str) {
    let Some(token) = url
        .strip_prefix(SCHEME)
        .and_then(|rest| rest.strip_prefix("://localhost/"))
    else {
        return;
    };
    let mut tokens = TOKENS.lock_ignore_poison();
    let owner = tokens
        .iter()
        .find(|(_, minted)| minted.iter().any(|t| t == token))
        .map(|(session, _)| session.clone());
    let Some(owner) = owner else {
        return;
    };
    if let Some(minted) = tokens.get_mut(&owner) {
        minted.retain(|t| t != token);
        if minted.is_empty() {
            tokens.remove(&owner);
        }
    }
    drop(tokens);
    drop_token(token);
}

/// Drops every audio token of the session. Idempotent.
pub(super) fn release_session(session_id: &str) {
    let minted = TOKENS.lock_ignore_poison().remove(session_id).unwrap_or_default();
    for token in &minted {
        drop_token(token);
    }
}

/// The canonical paths of `paths`, each a readable track of the session, in the order given.
/// Any other path refuses the whole call (`UnknownTrack`). Blocking.
pub fn player_targets(session_id: &str, paths: &[String]) -> Result<Vec<PathBuf>, TagEditorError> {
    Ok(match_tracks(session_id, paths)?
        .into_iter()
        .map(|(_, canonical)| canonical)
        .collect())
}

/// Each requested path as (the session's track, its canonical path). No file is read under the
/// store's lock: the tracks are copied out first.
fn match_tracks(session_id: &str, paths: &[String]) -> Result<Vec<(PathBuf, PathBuf)>, TagEditorError> {
    let tracks: Vec<(PathBuf, PathBuf)> = loaded_tracks(session_id)?
        .into_iter()
        .filter_map(|track| std::fs::canonicalize(&track).ok().map(|canonical| (track, canonical)))
        .collect();
    paths
        .iter()
        .map(|path| {
            let wanted = std::fs::canonicalize(Path::new(path)).map_err(|_| TagEditorError::UnknownTrack)?;
            tracks
                .iter()
                .find(|(_, canonical)| *canonical == wanted)
                .cloned()
                .ok_or(TagEditorError::UnknownTrack)
        })
        .collect()
}

/// Opens `paths` in foobar2000 when it's installed, else each in its default app. Never waits
/// for the launch: foobar2000's goes to the main thread (`NSWorkspace`), as "Open with" does.
pub fn open_in_player<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) -> Result<(), TagEditorError> {
    if paths.is_empty() {
        return Ok(());
    }
    if let Some(player) = foobar2000() {
        return app
            .run_on_main_thread(move || {
                if let Err(e) = crate::file_system::open_with::open_paths_with(&paths, &player) {
                    log::warn!(target: LOG_TARGET, "foobar2000 didn't start: {e}");
                }
            })
            .map_err(|_| TagEditorError::PlayerUnavailable);
    }
    for path in paths {
        // The app's own "open file" (the E2E build records instead of launching).
        crate::commands::file_actions::open_path(path.to_string_lossy().into_owned())
            .map_err(|_| TagEditorError::PlayerUnavailable)?;
    }
    Ok(())
}

/// Where foobar2000 is installed, if it is. One LaunchServices lookup.
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
fn foobar2000() -> Option<PathBuf> {
    crate::file_system::open_with::installed_app_path(FOOBAR2000_BUNDLE_ID).filter(|path| path.is_dir())
}

/// Never off macOS, and never in the E2E build, which must not launch apps it can't close.
#[cfg(not(all(target_os = "macos", not(feature = "playwright-e2e"))))]
fn foobar2000() -> Option<PathBuf> {
    None
}
