//! The Disc tools: fill Disc from the folder a file sits in, or cut a Disc value (`CD2`) down to its number.

use std::cmp::Ordering;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{FileTags, TagField};
use crate::text::remove_diacritics;
use crate::tools::ToolContext;

/// A folder named for a disc: `CD 1`, `CD1`, `Disc 2`, `Disk 03`, `cd-4`; the number is group 1.
static DISC_FOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^\p{L}])(?:cd|dis[ck])[ \t]*[-_.]?[ \t]*(\d+)").expect("disc folder regex is valid")
});

/// The first number of a Disc value, with its `/total` when it has one.
static DISC_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)(?:[ \t]*/[ \t]*(\d+))?").expect("disc number regex is valid"));

fn folder_name(folder: &str) -> &str {
    Path::new(folder)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(folder)
}

/// The disc number a folder name spells (`Disc 02` is 2), or `None` when it names no disc.
fn disc_in_name(name: &str) -> Option<u32> {
    DISC_FOLDER.captures(name)?.get(1)?.as_str().parse().ok()
}

/// Folder names in natural order: case and diacritics ignored, digit runs by value (`CD 2` before `CD 10`).
fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn chunks(text: &str) -> Vec<(bool, String)> {
        let folded = remove_diacritics(text).to_lowercase();
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in folded.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((is_digit, run)) if *is_digit == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (left, right) = (chunks(a), chunks(b));
    for ((a_digit, a_run), (b_digit, b_run)) in left.iter().zip(&right) {
        let order = if *a_digit && *b_digit {
            let (a_num, b_num) = (a_run.trim_start_matches('0'), b_run.trim_start_matches('0'));
            a_num.len().cmp(&b_num.len()).then_with(|| a_num.cmp(b_num))
        } else {
            a_run.cmp(b_run)
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

/// Sets Disc from the folder each file is in (`ctx.folders`): a folder named `CD 1` / `Disc 2` / `Disk 3`
/// gives that number; any other folder gets its place (1, 2, ...) among the distinct folders of the run in
/// natural order. Files all in one folder that names no disc are left alone: a flat book has no discs to
/// tell apart. A file with no folder in `ctx` is left alone.
pub fn disc_from_folder(files: &[FileTags], ctx: &ToolContext) -> Vec<FileTags> {
    let mut folders: Vec<&str> = ctx.folders.iter().map(String::as_str).collect();
    folders.sort_by(|a, b| natural_cmp(folder_name(a), folder_name(b)));
    folders.dedup();
    files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let mut out = file.clone();
            let Some(folder) = ctx.folders.get(i) else { return out };
            let number = disc_in_name(folder_name(folder)).or_else(|| {
                let place = folders.iter().position(|f| f == folder)?;
                (folders.len() > 1).then(|| u32::try_from(place + 1).unwrap_or(u32::MAX))
            });
            if let Some(number) = number {
                out.fields.insert(TagField::Disc, vec![number.to_string()]);
            }
            out
        })
        .collect()
}

/// Cuts every Disc value down to its number: `CD2` is `2`, `Disk 10/12` is `10/12`. A value with no digits
/// stays as it is.
pub fn disc_number_only(files: &[FileTags]) -> Vec<FileTags> {
    files
        .iter()
        .map(|file| {
            let mut out = file.clone();
            if let Some(values) = out.fields.get_mut(&TagField::Disc) {
                for value in values.iter_mut() {
                    if let Some(found) = DISC_NUMBER.captures(value) {
                        let number = found.get(1).map_or("", |m| m.as_str());
                        *value = match found.get(2) {
                            Some(total) => format!("{number}/{}", total.as_str()),
                            None => number.to_string(),
                        };
                    }
                }
            }
            out
        })
        .collect()
}
