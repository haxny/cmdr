//! Playing a session's tracks in the app, and opening them in a player (the system default app, or one the
//! user picked: the `multiRename.tagEditor.player` setting, `"default"` or a bundle id).
//!
//! Playback goes through the viewer's `cmdr-media://` scheme (`file_viewer/media.rs`): a token is
//! minted only for a readable track of the tag session (compared by canonical path), never for a
//! path the frontend made up. This module keeps the tokens it minted per session and drops them on
//! release and when the tag session closes (`session::close`), so no token outlives its sheet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use cmdr_tags::format_of;
use serde::Serialize;
use tauri::{AppHandle, Runtime};

use super::session::{TagEditorError, is_open, loaded_tracks};
use crate::file_viewer::content_kind::{ViewerContentKind, audio_mime};
use crate::file_viewer::media::{MediaEntry, drop_token, mint_token};
use crate::file_viewer::media_protocol::SCHEME;
use crate::ignore_poison::IgnorePoison;

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

/// What the player setting says for "the app each file opens with".
pub const DEFAULT_PLAYER: &str = "default";

/// An app the "Open in" menu offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppChoice {
    pub bundle_id: String,
    pub name: String,
    /// The system's default app for the tracks (the first candidate).
    pub is_default: bool,
}

/// How a launch goes, once the setting is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlan {
    /// Each file in its default app. `fell_back`: the user chose an app that isn't installed any more.
    Default { fell_back: bool },
    /// All the files in one launch of this app.
    With(PathBuf),
}

/// What `open_tags_in_player` answers: the launch was asked for, and whether the chosen app was missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerOpened {
    /// The chosen app isn't installed, so the default app was used.
    pub fell_back: bool,
}

/// Reads the setting (`"default"` or a bundle id); `installed` finds an app by bundle id.
pub fn plan_launch(player: &str, installed: impl Fn(&str) -> Option<PathBuf>) -> LaunchPlan {
    if player == DEFAULT_PLAYER {
        return LaunchPlan::Default { fell_back: false };
    }
    match installed(player) {
        Some(app) => LaunchPlan::With(app),
        None => LaunchPlan::Default { fell_back: true },
    }
}

/// Whether one app is the system default of every sampled type: only then can the button name it.
/// `firsts` is each type's default app (`None`: the type has none).
pub fn defaults_agree(firsts: &[Option<String>]) -> bool {
    match firsts.split_first() {
        Some((Some(first), rest)) => rest.iter().all(|other| other.as_ref() == Some(first)),
        _ => false,
    }
}

/// The menu's apps: the candidates in order (opening every type), then `chosen` when it isn't among
/// them (an app picked with "Choose…" that doesn't open these tracks by itself). The first candidate
/// is marked the system default only when `default_agrees` (every type has the same default app).
pub fn choices_of(
    candidates: &[crate::file_system::open_with::AppCandidate],
    chosen: Option<AppChoice>,
    default_agrees: bool,
) -> Vec<AppChoice> {
    let mut choices: Vec<AppChoice> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| AppChoice {
            bundle_id: c.bundle_id.clone(),
            name: c.display_name.clone(),
            is_default: default_agrees && i == 0,
        })
        .collect();
    if let Some(chosen) = chosen
        && !choices.iter().any(|c| c.bundle_id == chosen.bundle_id)
    {
        choices.push(AppChoice {
            is_default: false,
            ..chosen
        });
    }
    choices
}

/// Opens `paths` per the player setting. Never waits for the launch: an app launch goes to the main
/// thread (`NSWorkspace`), as "Open with" does. A chosen app that's gone falls back to each file's
/// default app, and the answer says so.
pub fn open_in_player<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<PathBuf>,
    player: &str,
) -> Result<PlayerOpened, TagEditorError> {
    if paths.is_empty() {
        return Ok(PlayerOpened { fell_back: false });
    }
    match plan_launch(player, installed_app) {
        LaunchPlan::With(player_app) => {
            app.run_on_main_thread(move || {
                if let Err(e) = crate::file_system::open_with::open_paths_with(&paths, &player_app) {
                    log::warn!(target: LOG_TARGET, "the player didn't start: {e}");
                }
            })
            .map_err(|_| TagEditorError::PlayerUnavailable)?;
            Ok(PlayerOpened { fell_back: false })
        }
        LaunchPlan::Default { fell_back } => {
            for path in paths {
                // The app's own "open file" (the E2E build records instead of launching).
                crate::commands::file_actions::open_path(path.to_string_lossy().into_owned())
                    .map_err(|_| TagEditorError::PlayerUnavailable)?;
            }
            Ok(PlayerOpened { fell_back })
        }
    }
}

/// The apps that can open every type of the session's tracks, default first when all agree on it, plus `chosen` (a bundle id) when it
/// is installed. One representative file per extension: LaunchServices answers per type. Blocking.
pub fn player_choices(session_id: &str, chosen: &str) -> Result<Vec<AppChoice>, TagEditorError> {
    let mut seen = std::collections::HashSet::new();
    let sample: Vec<PathBuf> = loaded_tracks(session_id)?
        .into_iter()
        .filter(|track| seen.insert(track.extension().map(|e| e.to_ascii_lowercase())))
        .collect();
    // Each type's own default app, to know whether one app is everyone's default.
    let firsts: Vec<Option<String>> = sample
        .iter()
        .map(|file| {
            candidates_of(vec![file.clone()])
                .into_iter()
                .next()
                .map(|candidate| candidate.bundle_id)
        })
        .collect();
    let candidates = candidates_of(sample);
    let chosen = (chosen != DEFAULT_PLAYER).then(|| choice_of_bundle(chosen)).flatten();
    Ok(choices_of(&candidates, chosen, defaults_agree(&firsts)))
}

/// Asks the user for an app with the system's open panel. `None` when they cancel (and always in the
/// E2E build, which opens no panel). Runs on the main thread, as the panel needs.
pub async fn choose_player<R: Runtime>(app: &AppHandle<R>) -> Result<Option<AppChoice>, TagEditorError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(pick_player());
    })
    .map_err(|_| TagEditorError::PlayerUnavailable)?;
    rx.await.map_err(|_| TagEditorError::PlayerUnavailable)
}

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
fn installed_app(bundle_id: &str) -> Option<PathBuf> {
    crate::file_system::open_with::installed_app_path(bundle_id).filter(|path| path.is_dir())
}

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
fn candidates_of(sample: Vec<PathBuf>) -> Vec<crate::file_system::open_with::AppCandidate> {
    crate::file_system::open_with::compute_open_with_choices(sample).candidates
}

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
fn choice_of_bundle(bundle_id: &str) -> Option<AppChoice> {
    let path = installed_app(bundle_id)?;
    Some(AppChoice {
        bundle_id: bundle_id.to_string(),
        name: crate::file_system::open_with::read_app_display_name(&path),
        is_default: false,
    })
}

#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
fn pick_player() -> Option<AppChoice> {
    let path = crate::file_system::open_with::pick_app_via_open_panel()?;
    let bundle_id = crate::file_system::open_with::read_bundle_identifier(&path)?;
    Some(AppChoice {
        bundle_id,
        name: crate::file_system::open_with::read_app_display_name(&path),
        is_default: false,
    })
}

/// Never off macOS, and never in the E2E build, which must not launch apps it can't close or open panels.
#[cfg(not(all(target_os = "macos", not(feature = "playwright-e2e"))))]
fn installed_app(_bundle_id: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(all(target_os = "macos", not(feature = "playwright-e2e"))))]
fn candidates_of(_sample: Vec<PathBuf>) -> Vec<crate::file_system::open_with::AppCandidate> {
    Vec::new()
}

#[cfg(not(all(target_os = "macos", not(feature = "playwright-e2e"))))]
fn choice_of_bundle(_bundle_id: &str) -> Option<AppChoice> {
    None
}

#[cfg(not(all(target_os = "macos", not(feature = "playwright-e2e"))))]
fn pick_player() -> Option<AppChoice> {
    None
}
