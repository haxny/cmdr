//! What counts as a name's extension: the ONE rule the whole product uses.
//!
//! In short: a folder has none unless it's a macOS package (`Cmdr.app`); a
//! file's is 1–10 ASCII letters and digits after the last dot, or a known
//! compressed tar (`tar.gz`); a leading or trailing dot isn't one. The rule, who
//! asks it, the TypeScript mirror, and what deliberately doesn't use it:
//! `crates/cmdr-fs/DETAILS.md` § "What counts as an extension".

/// Folder suffixes that make macOS present the folder as one opaque item, so
/// for a folder they're the only extensions there are. Lowercase, no dot. Hand
/// kept until the backend reports packages itself (issue #385).
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "component",
    "framework",
    "kext",
    "mdimporter",
    "photoslibrary",
    "pkg",
    "plugin",
    "prefpane",
    "qlgenerator",
    "rtfd",
    "saver",
    "wdgt",
    "xcodeproj",
    "xcworkspace",
];

/// Two-part extensions that count as one: the compressed tars whose
/// `ArchiveFormat::label` carries a dot. Lowercase, no leading dot.
pub const COMPOUND_EXTENSIONS: &[&str] = &["tar.gz", "tar.bz2", "tar.xz", "tar.zst"];

/// Longest plain extension, in ASCII characters.
const MAX_EXTENSION_LEN: usize = 10;

/// The extension of `name`, without its dot and in the name's own case, or
/// `None` when it has none under the product's rule (see the module docs).
pub fn extension_of(name: &str, is_directory: bool) -> Option<&str> {
    if is_directory {
        return package_extension(name);
    }
    compound_extension(name).or_else(|| plain_extension(name))
}

/// `name` with its extension and the dot before it stripped, or the whole name
/// when it has none.
pub fn name_without_extension(name: &str, is_directory: bool) -> &str {
    match extension_of(name, is_directory) {
        Some(ext) => &name[..name.len() - ext.len() - 1],
        None => name,
    }
}

/// The part after the last dot, when there's a stem before it.
fn last_segment(name: &str) -> Option<&str> {
    let dot = name.rfind('.').filter(|&dot| dot > 0)?;
    Some(&name[dot + 1..])
}

fn package_extension(name: &str) -> Option<&str> {
    last_segment(name).filter(|ext| PACKAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

fn plain_extension(name: &str) -> Option<&str> {
    last_segment(name)
        .filter(|ext| (1..=MAX_EXTENSION_LEN).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn compound_extension(name: &str) -> Option<&str> {
    COMPOUND_EXTENSIONS.iter().find_map(|compound| {
        // A real stem before the dot, so `.tar.gz` alone is a dotfile's `gz`.
        let dot = name.len().checked_sub(compound.len() + 1).filter(|&dot| dot > 0)?;
        let tail = name.get(dot..)?;
        let ext = tail.strip_prefix('.')?;
        ext.eq_ignore_ascii_case(compound).then_some(ext)
    })
}

#[cfg(test)]
#[path = "file_extension_test.rs"]
mod tests;
