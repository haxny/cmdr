//! Track and disc totals: which items of a file hold one (each a [`Mapped`] item of
//! `TrackTotal` / `DiscTotal`, so [`crate::read::hidden_items`] names the ones the reader
//! doesn't show and a write refuses to drop them silently), the total the reader shows, and the
//! total a write puts beside its number.
//!
//! An item's value is the total as text (`012` reads as `12`), or its text as is when it isn't
//! a number, which then never matches a shown total.

use std::collections::BTreeMap;

use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst};
use lofty::ogg::tag::VorbisComments;

use crate::model::{FileTags, TagError, TagField};
use crate::read::{Mapped, fourcc_name};

/// Each total field with the number it is written beside.
const TOTALS: [(TagField, TagField); 2] = [
    (TagField::Track, TagField::TrackTotal),
    (TagField::Disc, TagField::DiscTotal),
];

/// A total item `id` of `field` from `text`; none for an empty text or a zero (no total).
fn total_item(id: impl Into<String>, field: TagField, text: &str) -> Option<Mapped> {
    let text = text.trim();
    let value = match text.parse::<u32>() {
        Ok(0) => return None,
        Ok(total) => total.to_string(),
        Err(_) if text.is_empty() => return None,
        Err(_) => text.to_owned(),
    };
    Some(Mapped {
        id: id.into(),
        field,
        values: vec![value],
        fallback: false,
    })
}

/// The total item in a `"N/T"` number; none for a plain number.
fn slash_total(id: impl Into<String>, field: TagField, value: &str) -> Option<Mapped> {
    let (_, total) = value.split_once('/')?;
    total_item(id, field, total)
}

/// Puts the totals among `items` in `fields`: for each, the first item whose value is a number,
/// and only beside its number (a write drops a total without its number, so the view doesn't
/// show one either; [`crate::read::hidden_items`] then names it).
pub(crate) fn add_totals(fields: &mut BTreeMap<TagField, Vec<String>>, items: &[Mapped]) {
    for (number, total) in TOTALS {
        let shown = items
            .iter()
            .filter(|item| item.field == total)
            .flat_map(|item| item.values.first())
            .find(|value| value.parse::<u32>().is_ok());
        if let Some(value) = shown.filter(|_| fields.contains_key(&number)) {
            fields.insert(total, vec![value.clone()]);
        }
    }
}

/// The totals of `id3`: `TRCK`'s and `TPOS`'s `/T`, then the legacy `TXXX:DiscNumber`'s.
pub(crate) fn id3_total_items(id3: &Id3v2Tag) -> Vec<Mapped> {
    let mut legacy = Vec::new();
    let mut items = Vec::new();
    for frame in id3 {
        match frame {
            Frame::Text(f) => {
                let field = match f.id().as_str() {
                    "TRCK" => TagField::TrackTotal,
                    "TPOS" => TagField::DiscTotal,
                    _ => continue,
                };
                items.extend(slash_total(f.id().as_str(), field, &f.value));
            }
            Frame::UserText(f) if f.description.eq_ignore_ascii_case("DiscNumber") => {
                legacy.extend(slash_total(
                    format!("TXXX:{}", f.description),
                    TagField::DiscTotal,
                    &f.content,
                ));
            }
            _ => {}
        }
    }
    items.extend(legacy);
    items
}

/// The totals of `ilst`: bytes 4-5 of a `trkn` / `disk` atom's data, 0 meaning none.
pub(crate) fn ilst_total_items(ilst: &Ilst) -> Vec<Mapped> {
    [(*b"trkn", TagField::TrackTotal), (*b"disk", TagField::DiscTotal)]
        .into_iter()
        .filter_map(
            |(fourcc, field)| match ilst.get(&AtomIdent::Fourcc(fourcc)).and_then(|a| a.data().next()) {
                Some(AtomData::Unknown { data, .. }) if data.len() >= 6 => {
                    let total = u16::from_be_bytes([data[4], data[5]]);
                    total_item(fourcc_name(&fourcc), field, &total.to_string())
                }
                _ => None,
            },
        )
        .collect()
}

/// The totals of `comments`: the total keys first (a key wins over `N/T`), then the `N/T`
/// numbers, each by its uppercase key.
pub(crate) fn vorbis_total_items(comments: &VorbisComments) -> Vec<Mapped> {
    let keyed = comments.items().filter_map(|(key, value)| {
        let key = key.to_ascii_uppercase();
        let field = match key.as_str() {
            "TRACKTOTAL" | "TOTALTRACKS" => TagField::TrackTotal,
            "DISCTOTAL" | "TOTALDISCS" => TagField::DiscTotal,
            _ => return None,
        };
        total_item(key, field, value)
    });
    let slashed = comments.items().filter_map(|(key, value)| {
        let key = key.to_ascii_uppercase();
        let field = match key.as_str() {
            "TRACKNUMBER" => TagField::TrackTotal,
            "DISCNUMBER" => TagField::DiscTotal,
            _ => return None,
        };
        slash_total(key, field, value)
    });
    keyed.chain(slashed).collect()
}

/// The total a write puts beside the number of `field`: the field's value, else `fallback`
/// (`WriteOptions`) when the field is absent; none when the number isn't written or the total
/// is zero (MP4 stores none as zero). A total that isn't a whole number fails the write.
pub(crate) fn total_to_write(
    tags: &FileTags,
    values: &BTreeMap<TagField, Vec<String>>,
    field: TagField,
    fallback: Option<u32>,
) -> Result<Option<u32>, TagError> {
    let number_written = TOTALS
        .iter()
        .any(|(number, total)| *total == field && values.contains_key(number));
    if !number_written {
        return Ok(None);
    }
    let Some(text) = tags
        .fields
        .get(&field)
        .and_then(|v| v.iter().map(|t| t.trim()).find(|t| !t.is_empty()))
    else {
        return Ok(fallback.filter(|t| *t > 0));
    };
    text.parse::<u32>()
        .map(|t| Some(t).filter(|t| *t > 0))
        .map_err(|e| TagError::Write(format!("{text:?} isn't a track or disc total: {e}")))
}

/// Puts each total in `fields` beside its number, as the reader shows it.
pub(crate) fn put_totals(fields: &mut BTreeMap<TagField, Vec<String>>, track: Option<u32>, disc: Option<u32>) {
    for ((number, field), total) in TOTALS.into_iter().zip([track, disc]) {
        if let Some(total) = total.filter(|_| fields.contains_key(&number)) {
            fields.insert(field, vec![total.to_string()]);
        }
    }
}
