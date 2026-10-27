//! The foobar2000-style view over several files at once: what they share, the edits a user
//! makes against that view, and the list of what an apply would change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{Cover, FileTags, TagField};

/// What one field looks like across the selected files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SharedValue {
    /// No file has the field.
    Empty,
    /// Every file has exactly these values.
    Same(Vec<String>),
    /// The files differ, or only some have the field.
    Multiple,
}

/// What to do with the front cover of every file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum CoverEdit {
    /// Leave covers as they are.
    Keep,
    /// Give every file this cover.
    Set(Cover),
    /// Remove every cover.
    Remove,
}

/// One user edit against the shared view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Edit {
    /// The shared row was edited: set the field on every file. Empty values remove it.
    #[serde(rename_all = "camelCase")]
    SetAll {
        /// The field.
        field: TagField,
        /// The new values.
        values: Vec<String>,
    },
    /// One file's entry in the per-track list was edited.
    #[serde(rename_all = "camelCase")]
    SetOne {
        /// Index into the files; out of range is ignored.
        row: usize,
        /// The field.
        field: TagField,
        /// The new values.
        values: Vec<String>,
    },
    /// The cover was edited, for all files.
    #[serde(rename_all = "camelCase")]
    Cover {
        /// What to do with it.
        edit: CoverEdit,
    },
}

/// One distinct value set of a field and how many files carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ValueCount {
    /// The field's values in one file (several for multi-value fields).
    pub values: Vec<String>,
    /// How many files have exactly these values.
    pub files: u32,
}

/// What an apply would change in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Index into the files.
    pub row: usize,
    /// The managed fields that differ, in display order.
    pub fields: Vec<TagField>,
    /// Whether the cover differs.
    pub cover: bool,
    /// Whether the unmanaged extras differ.
    pub extras: bool,
}

/// What each managed field looks like across `files`. Every field is a key.
pub fn shared_view(files: &[FileTags]) -> BTreeMap<TagField, SharedValue> {
    TagField::ALL
        .iter()
        .map(|&field| (field, shared_value(files, field)))
        .collect()
}

fn shared_value(files: &[FileTags], field: TagField) -> SharedValue {
    let Some((first, rest)) = files.split_first() else {
        return SharedValue::Empty;
    };
    let first_values = first.fields.get(&field);
    if rest.iter().any(|f| f.fields.get(&field) != first_values) {
        return SharedValue::Multiple;
    }
    match first_values {
        Some(values) => SharedValue::Same(values.clone()),
        None => SharedValue::Empty,
    }
}

/// The distinct value sets of `field` across `files` with their file counts, most common first
/// (ties in order of first appearance), at most `limit`. Files without the field aren't counted.
pub fn distinct_values(files: &[FileTags], field: TagField, limit: usize) -> Vec<ValueCount> {
    let mut counts: Vec<ValueCount> = Vec::new();
    for values in files.iter().filter_map(|f| f.fields.get(&field)) {
        match counts.iter_mut().find(|c| &c.values == values) {
            Some(count) => count.files += 1,
            None => counts.push(ValueCount {
                values: values.clone(),
                files: 1,
            }),
        }
    }
    counts.sort_by_key(|c| std::cmp::Reverse(c.files)); // stable: ties keep first appearance
    counts.truncate(limit);
    counts
}

/// Applies `edits` in order to a copy of `files`. Values are trimmed and blanks dropped; a field
/// left with no values is removed.
pub fn apply_edits(files: &[FileTags], edits: &[Edit]) -> Vec<FileTags> {
    edits.iter().fold(files.to_vec(), apply_edit)
}

fn apply_edit(files: Vec<FileTags>, edit: &Edit) -> Vec<FileTags> {
    match edit {
        Edit::SetAll { field, values } => files.into_iter().map(|f| with_field(f, *field, values)).collect(),
        Edit::SetOne { row, field, values } => files
            .into_iter()
            .enumerate()
            .map(|(i, f)| if i == *row { with_field(f, *field, values) } else { f })
            .collect(),
        Edit::Cover { edit } => files.into_iter().map(|f| with_cover(f, edit)).collect(),
    }
}

fn with_field(mut tags: FileTags, field: TagField, values: &[String]) -> FileTags {
    let cleaned: Vec<String> = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect();
    if cleaned.is_empty() {
        tags.fields.remove(&field);
    } else {
        tags.fields.insert(field, cleaned);
    }
    tags
}

fn with_cover(mut tags: FileTags, edit: &CoverEdit) -> FileTags {
    match edit {
        CoverEdit::Keep => {}
        CoverEdit::Set(cover) => tags.cover = Some(cover.clone()),
        CoverEdit::Remove => tags.cover = None,
    }
    tags
}

/// The rows that differ between `before` and `after`. If the lengths differ, rows beyond the
/// shorter list count as changed in every field they hold.
pub fn changes(before: &[FileTags], after: &[FileTags]) -> Vec<FileChange> {
    let empty = FileTags::default();
    (0..before.len().max(after.len()))
        .filter_map(|row| {
            let old = before.get(row).unwrap_or(&empty);
            let new = after.get(row).unwrap_or(&empty);
            let change = FileChange {
                row,
                fields: TagField::ALL
                    .iter()
                    .copied()
                    .filter(|f| old.fields.get(f) != new.fields.get(f))
                    .collect(),
                cover: old.cover != new.cover,
                extras: old.extras != new.extras,
            };
            (!change.fields.is_empty() || change.cover || change.extras).then_some(change)
        })
        .collect()
}
