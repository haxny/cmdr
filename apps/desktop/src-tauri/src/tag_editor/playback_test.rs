//! Playing a session's tracks in the app: a token is minted only for a loaded track of the session,
//! resolves to its canonical path with the format's audio type, and dies on release and on close.

use std::path::PathBuf;

use cmdr_tags::AudioFormat;

use super::playback::{audio_url, player_targets, release};
use super::session::{TagEditorError, close, open};
use super::session_test::{album, path_of};
use crate::file_viewer::content_kind::{ViewerContentKind, audio_mime};
use crate::file_viewer::media::{MediaEntry, mint_token, resolve_token};
use crate::multi_rename::session::close as close_rename;

fn token_of(url: &str) -> &str {
    url.strip_prefix("cmdr-media://localhost/").expect("a cmdr-media URL")
}

fn resolved(url: &str) -> Option<MediaEntry> {
    resolve_token(token_of(url)).and_then(|source| source.ready().cloned())
}

#[test]
fn url_only_for_session_tracks() {
    let (dir, _listing, id) = album("tag-play-only-tracks");
    open(&id, false).expect("opens");
    assert_eq!(audio_url(&id, "/etc/hosts"), Err(TagEditorError::UnknownTrack));
    // In the folder, but not a track of the session.
    assert_eq!(
        audio_url(&id, &path_of(&dir, "cover.jpg")),
        Err(TagEditorError::UnknownTrack)
    );
    assert_eq!(
        audio_url(&id, &path_of(&dir, "missing.mp3")),
        Err(TagEditorError::UnknownTrack)
    );
    assert_eq!(
        audio_url("no-such-session", &path_of(&dir, "a.mp3")),
        Err(TagEditorError::NoSession)
    );
    close(&id);
    close_rename(&id);
}

#[test]
fn url_for_loaded_track_resolves() {
    let (dir, _listing, id) = album("tag-play-resolves");
    open(&id, false).expect("opens");
    // A spelling of the path that isn't the session's own still names the same track.
    let spelled = dir.join(".").join("a.mp3").to_string_lossy().into_owned();
    let url = audio_url(&id, &spelled).expect("a track of the session gets a URL");
    assert!(url.starts_with("cmdr-media://localhost/"), "{url}");
    let entry = resolved(&url).expect("the token resolves");
    let canonical: PathBuf = std::fs::canonicalize(dir.join("a.mp3")).expect("the file exists");
    assert_eq!(entry.canonical_path, canonical);
    assert_eq!(entry.kind, ViewerContentKind::Audio);
    assert_eq!(entry.mime, "audio/mpeg");
    close(&id);
    close_rename(&id);
}

#[test]
fn release_drops_token() {
    let (dir, _listing, id) = album("tag-play-release");
    open(&id, false).expect("opens");
    let url = audio_url(&id, &path_of(&dir, "b.mp3")).expect("a URL");
    release(&url);
    assert!(resolved(&url).is_none(), "a released token is gone");
    release(&url); // idempotent
    close(&id);
    close_rename(&id);
}

#[test]
fn release_leaves_tokens_it_did_not_mint() {
    let token = mint_token(MediaEntry {
        canonical_path: PathBuf::from("/tmp/viewer.png"),
        kind: ViewerContentKind::Image,
        mime: "image/png".to_string(),
    });
    release(&format!("cmdr-media://localhost/{token}"));
    assert!(
        resolve_token(&token).is_some(),
        "a viewer's token is not the tag editor's to drop"
    );
    crate::file_viewer::media::drop_token(&token);
}

#[test]
fn audio_mime_by_format() {
    assert_eq!(audio_mime(AudioFormat::Mp3), "audio/mpeg");
    assert_eq!(audio_mime(AudioFormat::Mp4), "audio/mp4");
    assert_eq!(audio_mime(AudioFormat::Flac), "audio/flac");
    assert_eq!(audio_mime(AudioFormat::OggVorbis), "audio/ogg");
    assert_eq!(audio_mime(AudioFormat::Opus), "audio/ogg");
}

#[test]
fn closing_session_drops_its_tokens() {
    let (dir, _listing, id) = album("tag-play-close");
    open(&id, false).expect("opens");
    let a = audio_url(&id, &path_of(&dir, "a.mp3")).expect("a URL");
    let c = audio_url(&id, &path_of(&dir, "c.mp3")).expect("a URL");
    tauri::async_runtime::block_on(crate::commands::tag_editor::close_tag_editor(id.clone()));
    assert!(resolved(&a).is_none(), "closing the tag session drops its tokens");
    assert!(resolved(&c).is_none(), "closing the tag session drops its tokens");
    close_rename(&id);
}

#[test]
fn player_targets_only_session_tracks() {
    let (dir, _listing, id) = album("tag-play-player");
    open(&id, false).expect("opens");
    let targets = player_targets(&id, &[path_of(&dir, "a.mp3"), path_of(&dir, "c.mp3")]).expect("tracks");
    let canonical = |name: &str| std::fs::canonicalize(dir.join(name)).expect("the file exists");
    assert_eq!(targets, vec![canonical("a.mp3"), canonical("c.mp3")]);
    assert_eq!(
        player_targets(&id, &[path_of(&dir, "a.mp3"), path_of(&dir, "notes.txt")]),
        Err(TagEditorError::UnknownTrack)
    );
    close(&id);
    close_rename(&id);
}
