//! What the Tags window did, in the log: one `info` line per user action (counts and durations),
//! `debug` for the detail. ❌ Never a path or a tag value: errors go in by kind only (a
//! `TagError::Write` message can quote a value).

use std::collections::BTreeMap;
use std::time::Instant;

use cmdr_tags::{RestoreReport, TagError};

use super::apply::TagApplyReport;
use super::session::{FileChangeView, TagEditorError};
use super::view::TagEditorView;

const LOG_TARGET: &str = "tag_editor";

/// After `open`: how many tracks, how many could be read, and how long it took.
pub(crate) fn opened(result: &Result<TagEditorView, TagEditorError>, started: Instant) {
    let ms = started.elapsed().as_millis();
    match result {
        Ok(view) => {
            let readable = view.tracks.iter().filter(|track| track.read_error.is_none()).count();
            log::info!(target: LOG_TARGET, "open: {} tracks, {readable} readable, in {ms} ms", view.tracks.len());
        }
        Err(error) => refused("open", error, ms),
    }
}

/// After an edit, a tool or a text tool (`action`): how many tracks the view holds, how many
/// differ from the disk, and how long it took.
pub(crate) fn edited(action: &str, result: &Result<TagEditorView, TagEditorError>, started: Instant) {
    let ms = started.elapsed().as_millis();
    match result {
        Ok(view) => {
            let changed = view.tracks.iter().filter(|track| track.changed).count();
            log::info!(
                target: LOG_TARGET,
                "{action}: {} files, {changed} changed, in {ms} ms",
                view.tracks.len()
            );
        }
        Err(error) => refused(action, error, ms),
    }
}

/// After the change list a write starts from.
pub(crate) fn listed(result: &Result<Vec<FileChangeView>, TagEditorError>, started: Instant) {
    let ms = started.elapsed().as_millis();
    match result {
        Ok(changes) => log::info!(target: LOG_TARGET, "changes: {} files, in {ms} ms", changes.len()),
        Err(error) => refused("changes", error, ms),
    }
}

/// After a write: what it did per outcome, and why files weren't written, by kind.
pub(crate) fn applied(result: &Result<TagApplyReport, TagEditorError>, started: Instant) {
    let ms = started.elapsed().as_millis();
    let report = match result {
        Ok(report) => report,
        Err(error) => return refused("apply", error, ms),
    };
    log::info!(
        target: LOG_TARGET,
        "apply: {} written, {} failed, {} would drop, {} changed on disk, in {ms} ms",
        report.written.len(),
        report.failed.len(),
        report.would_drop.len(),
        report.changed_on_disk.len()
    );
    if !report.failed.is_empty() {
        let by_kind = counts(report.failed.iter().map(|row| tag_error_kind(&row.error)));
        log::debug!(target: LOG_TARGET, "apply: not written, by kind: {by_kind:?}");
    }
    if report.undo_unavailable {
        log::info!(target: LOG_TARGET, "apply: this write can't be undone");
    }
}

/// After an undo.
pub(crate) fn undone(result: &Result<RestoreReport, TagEditorError>, started: Instant) {
    let ms = started.elapsed().as_millis();
    match result {
        Ok(report) => log::info!(
            target: LOG_TARGET,
            "undo: {} restored, {} missing, {} changed since, {} failed, in {ms} ms",
            report.restored.len(),
            report.missing.len(),
            report.changed.len(),
            report.failed.len()
        ),
        Err(error) => refused("undo", error, ms),
    }
}

fn refused(action: &str, error: &TagEditorError, ms: u128) {
    log::info!(target: LOG_TARGET, "{action}: refused ({}), in {ms} ms", error_kind(error));
}

fn counts<'a>(kinds: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    kinds.fold(BTreeMap::new(), |mut counted, kind| {
        *counted.entry(kind).or_default() += 1;
        counted
    })
}

/// The error's kind, never its data (which can hold a path or a tag value).
fn error_kind(error: &TagEditorError) -> &'static str {
    match error {
        TagEditorError::NoSession => "noSession",
        TagEditorError::NotOpen => "notOpen",
        TagEditorError::ReadOnly => "readOnly",
        TagEditorError::UnknownTrack => "unknownTrack",
        TagEditorError::Undo { .. } => "undo",
        TagEditorError::NoAppDataDir => "noAppDataDir",
        TagEditorError::BadCoverName => "badCoverName",
        TagEditorError::CoverTooLarge => "coverTooLarge",
        TagEditorError::CoverUnreadable => "coverUnreadable",
        TagEditorError::CoverNotAnImage => "coverNotAnImage",
        TagEditorError::PastedNotAnImage => "pastedNotAnImage",
        TagEditorError::CoverTimedOut => "coverTimedOut",
        TagEditorError::CoverBusy => "coverBusy",
        TagEditorError::PasteTimedOut => "pasteTimedOut",
        TagEditorError::CoverNotAFile => "coverNotAFile",
        TagEditorError::UnknownCover => "unknownCover",
        TagEditorError::ReadTimedOut => "readTimedOut",
        TagEditorError::TaskFailed => "taskFailed",
        TagEditorError::BadPattern { .. } => "badPattern",
        TagEditorError::BadRegex => "badRegex",
        TagEditorError::BadMask { .. } => "badMask",
        TagEditorError::PlayerUnavailable => "playerUnavailable",
    }
}

fn tag_error_kind(error: &TagError) -> &'static str {
    match error {
        TagError::NotSupported => "notSupported",
        TagError::Io(..) => "io",
        TagError::Parse(_) => "parse",
        TagError::Write(_) => "write",
        TagError::BadDate(_) => "badDate",
        TagError::WouldDrop(_) => "wouldDrop",
        TagError::UnwritableExtra(_) => "unwritableExtra",
    }
}
