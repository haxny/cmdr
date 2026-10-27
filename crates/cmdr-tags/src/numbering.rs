//! The auto-numbering wizard: numbers tracks and discs over the session's files, starting a new
//! disc where the folder or a field value changes, with totals and leading zeros.

use serde::{Deserialize, Serialize};

use crate::model::{FileTags, TagField};
use crate::tools::{ToolContext, pad_number, track_width};

/// What [`auto_number`] does with discs. Absent in [`NumberOptions`] means discs are left alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiscOptions {
    /// The first disc number.
    pub start: u32,
    /// Write DiscTotal (the number of discs in the run).
    pub total: bool,
    /// Pad disc numbers to at least two digits (`01`).
    pub pad: bool,
    /// A new disc starts where the folder changes.
    pub next_on_folder: bool,
    /// A new disc starts where this field's first value changes.
    pub next_on_field: Option<TagField>,
    /// Track numbers restart at the track start on each new disc.
    pub reset_track: bool,
    /// Disc numbers restart at `start` where this field's first value changes.
    pub reset_disc_on_field: Option<TagField>,
}

/// What [`auto_number`] does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct NumberOptions {
    /// The first track number.
    pub track_start: u32,
    /// Write TrackTotal.
    pub track_total: bool,
    /// Pad track numbers to the width of the biggest one (at least two digits).
    pub track_pad: bool,
    /// Disc numbering, or none to leave discs alone.
    pub disc: Option<DiscOptions>,
}

/// A run of files on one disc: `files` index range, the disc number and the disc-number
/// segment (discs between two resets) it belongs to.
struct Group {
    first: usize,
    end: usize,
    disc: u32,
    segment: usize,
}

fn first_value(file: &FileTags, field: TagField) -> Option<&String> {
    file.fields.get(&field).and_then(|v| v.first())
}

fn clamp(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Splits the files into disc groups. A boundary never comes before the first file.
fn group_files(files: &[FileTags], ctx: &ToolContext, disc: Option<&DiscOptions>) -> Vec<Group> {
    let mut groups = vec![Group {
        first: 0,
        end: files.len(),
        disc: disc.map_or(1, |d| d.start),
        segment: 0,
    }];
    let Some(opts) = disc else { return groups };
    let changed = |field: Option<TagField>, i: usize| {
        field.is_some_and(|f| first_value(&files[i], f) != first_value(&files[i - 1], f))
    };
    for i in 1..files.len() {
        let folder_changed = opts.next_on_folder && ctx.folders.get(i) != ctx.folders.get(i - 1);
        let reset = changed(opts.reset_disc_on_field, i);
        if !reset && !folder_changed && !changed(opts.next_on_field, i) {
            continue;
        }
        let Some(last) = groups.last_mut() else { continue };
        last.end = i;
        let (disc, segment) = if reset {
            (opts.start, last.segment + 1)
        } else {
            (last.disc.saturating_add(1), last.segment)
        };
        groups.push(Group {
            first: i,
            end: files.len(),
            disc,
            segment,
        });
    }
    groups
}

fn set(tags: &mut FileTags, field: TagField, text: String) {
    tags.fields.insert(field, vec![text]);
}

/// Numbers the files in order. Tracks count from `track_start` (restarting on each disc when
/// `reset_track`), padded and with TrackTotal as asked: the files on the disc when tracks
/// restart, else the files in the run. Discs count from `start`; DiscTotal is the number of
/// discs in the run (in the stretch between two disc resets, when `reset_disc_on_field` is
/// set). A single file gets no track number and no track total.
pub fn auto_number(files: &[FileTags], ctx: &ToolContext, opts: &NumberOptions) -> Vec<FileTags> {
    if files.is_empty() {
        return Vec::new();
    }
    let groups = group_files(files, ctx, opts.disc.as_ref());
    let restart = opts.disc.as_ref().is_some_and(|d| d.reset_track);
    let mut out = files.to_vec();
    let mut next_track = opts.track_start;
    for group in &groups {
        if restart {
            next_track = opts.track_start;
        }
        let count = group.end - group.first;
        let counted = if restart { count } else { files.len() };
        let last = opts.track_start.saturating_add(clamp(counted).saturating_sub(1));
        let width = if opts.track_pad {
            track_width(last.max(clamp(counted)))
        } else {
            0
        };
        for tags in &mut out[group.first..group.end] {
            if files.len() == 1 {
                tags.fields.remove(&TagField::Track);
                tags.fields.remove(&TagField::TrackTotal);
            } else {
                set(tags, TagField::Track, pad_number(next_track, width));
                if opts.track_total {
                    set(tags, TagField::TrackTotal, counted.to_string());
                }
            }
            next_track = next_track.saturating_add(1);
        }
    }
    if let Some(disc) = &opts.disc {
        write_discs(&mut out, &groups, disc);
    }
    out
}

fn write_discs(out: &mut [FileTags], groups: &[Group], opts: &DiscOptions) {
    for group in groups {
        let in_segment = groups.iter().filter(|g| g.segment == group.segment);
        let discs = clamp(in_segment.clone().count());
        let biggest = in_segment.map(|g| g.disc).max().unwrap_or(group.disc);
        let width = if opts.pad { track_width(discs.max(biggest)) } else { 0 };
        for tags in &mut out[group.first..group.end] {
            set(tags, TagField::Disc, pad_number(group.disc, width));
            if opts.total {
                set(tags, TagField::DiscTotal, discs.to_string());
            }
        }
    }
}
