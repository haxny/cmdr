//! AppleDouble sidecars: the `._name` bytes macOS archivers write beside an item
//! to carry its extended attributes, and where an archive keeps them.
//!
//! **Where they live.** `ditto -c -k --sequesterRsrc` (Finder's Compress) puts
//! each item's sidecar at `__MACOSX/<parent>/._<name>`, AFTER the item; a macOS
//! `tar -c` (bsdtar) writes it as the `._<name>` sibling right BEFORE the item
//! (verified with the checked-in fixtures, `crate::test_fixtures`). Both restore
//! them on extract and never show them, and so does Cmdr: the index pairs each
//! sidecar with its item (`read/index.rs`) and the copy carries what it holds as
//! the item's xattrs.
//!
//! **The layout** is the one `copyfile(3)` writes and reads (apple-oss-
//! distributions `copyfile.c`, `apple_double_header_t` / `attr_header_t`, all
//! big-endian): a 26-byte header, an entry table, the Finder Info entry (id 9)
//! whose 32 bytes are followed by two bytes of padding and an `ATTR` block
//! holding every other xattr, then the resource fork entry (id 2). Attribute
//! offsets in the `ATTR` block count from the start of the file.
//!
//! **Untrusted input.** A sidecar comes out of an archive, so [`parse`] trusts
//! no offset, length, or count: every read is bounds-checked with checked
//! arithmetic, a malformed part is skipped (never a panic, never an error), and a
//! name reaches a file only if the kernel would read it as the same name
//! ([`admissible_name`]). The caller bounds the input itself
//! ([`MAX_SIDECAR_BYTES`]).

#![allow(
    dead_code,
    reason = "the compress side, `build` and `worth_keeping`, is wired by the next commit"
)]

use std::ffi::OsString;

use cmdr_fs::metadata::Xattr;

/// The folder a Finder-made zip keeps its sidecars in, at the archive root.
pub(crate) const MACOSX_FOLDER: &str = "__MACOSX";

/// The largest sidecar read into memory. A sidecar's bytes are xattr VALUES, and
/// a resource fork can be large, so this bounds what one hostile entry can make
/// the reader buffer (a zip entry can claim any size and inflate to it). Above
/// it the sidecar is skipped and logged, the item lands without its xattrs.
pub(crate) const MAX_SIDECAR_BYTES: u64 = 16 * 1024 * 1024;

/// The 32 bytes of Finder Info: Finder flags (the custom-icon bit among them),
/// type and creator codes.
pub(crate) const FINDER_INFO_XATTR: &str = "com.apple.FinderInfo";

/// A file's resource fork, as macOS presents it through the xattr calls.
pub(crate) const RESOURCE_FORK_XATTR: &str = "com.apple.ResourceFork";

/// Stamped by the kernel on every file a process writes; it ignores a write of
/// it, so a sidecar copy of it is noise (`cmdr_fs::metadata::carried.rs`).
const PROVENANCE_XATTR: &str = "com.apple.provenance";

/// Gatekeeper's mark. Carried, but never by itself a reason to write a sidecar.
const QUARANTINE_XATTR: &str = "com.apple.quarantine";

/// The protected namespace (`com.apple.system.Security` is the ACL): the kernel
/// refuses a user write, so a sidecar naming one is either confused or hostile.
const SYSTEM_NAMESPACE: &str = "com.apple.system.";

/// `XATTR_MAXNAMELEN`: the longest name `setxattr(2)` takes, in bytes.
const MAX_NAME_BYTES: usize = 127;

const MAGIC: u32 = 0x0005_1607;
const VERSION: u32 = 0x0002_0000;
const FILLER: &[u8; 16] = b"Mac OS X        ";
const ENTRY_RESOURCE_FORK: u32 = 2;
const ENTRY_FINDER_INFO: u32 = 9;
const FINDER_INFO_LEN: usize = 32;
const ATTR_MAGIC: u32 = u32::from_be_bytes(*b"ATTR");

/// Header (26) + two entries (12 each): where copyfile puts the Finder Info.
const FINDER_INFO_OFFSET: usize = 26 + 2 * 12;
/// The `ATTR` block follows the Finder Info and two bytes of padding.
const ATTR_HEADER_OFFSET: usize = FINDER_INFO_OFFSET + FINDER_INFO_LEN + 2;
/// magic, debug tag, total size, data start, data length, three reserved, then
/// flags and the attribute count (u16 each).
const ATTR_HEADER_LEN: usize = 4 * 8 + 2 + 2;
/// offset, length (u32 each), flags (u16), name length (u8).
const ATTR_ENTRY_FIXED_LEN: usize = 4 + 4 + 2 + 1;

/// Reads the xattrs a sidecar carries: the Finder Info (unless all zero), the
/// resource fork (unless empty), and every admissible name in the `ATTR` block,
/// first occurrence winning. Provenance and the protected namespace never come
/// back. Total over any input: a malformed part is skipped.
pub(crate) fn parse(bytes: &[u8]) -> Vec<Xattr> {
    let mut out: Vec<Xattr> = Vec::new();
    if read_u32(bytes, 0) != Some(MAGIC) {
        return out;
    }
    let Some(entry_count) = read_u16(bytes, 24) else {
        return out;
    };
    // An entry is 12 bytes; a count past what the input holds stops at its end.
    for index in 0..usize::from(entry_count) {
        let Some(at) = index.checked_mul(12).and_then(|offset| offset.checked_add(26)) else {
            break;
        };
        let (Some(id), Some(offset), Some(length)) =
            (read_u32(bytes, at), read_u32(bytes, at + 4), read_u32(bytes, at + 8))
        else {
            break;
        };
        let Some(body) = slice(bytes, offset as usize, length as usize) else {
            continue;
        };
        match id {
            ENTRY_FINDER_INFO => {
                if let Some(finder_info) = body.get(..FINDER_INFO_LEN)
                    && finder_info.iter().any(|byte| *byte != 0)
                {
                    push_unique(&mut out, FINDER_INFO_XATTR, finder_info);
                }
                parse_attr_block(bytes, offset as usize, body, &mut out);
            }
            ENTRY_RESOURCE_FORK if !body.is_empty() => push_unique(&mut out, RESOURCE_FORK_XATTR, body),
            _ => {}
        }
    }
    out
}

/// The `ATTR` block inside the Finder Info entry `finder_info_entry`, which
/// starts at `entry_offset` in the sidecar: every xattr but the Finder Info and
/// the resource fork.
fn parse_attr_block(bytes: &[u8], entry_offset: usize, finder_info_entry: &[u8], out: &mut Vec<Xattr>) {
    let header_at = FINDER_INFO_LEN + 2;
    if read_u32(finder_info_entry, header_at) != Some(ATTR_MAGIC) {
        return;
    }
    let Some(count) = read_u16(finder_info_entry, header_at + ATTR_HEADER_LEN - 2) else {
        return;
    };
    // Entries follow the header; they're addressed within the Finder Info
    // entry, their data by absolute offset within the whole sidecar.
    let mut at = header_at + ATTR_HEADER_LEN;
    for _ in 0..count {
        let (Some(offset), Some(length), Some(name_len)) = (
            read_u32(finder_info_entry, at),
            read_u32(finder_info_entry, at + 4),
            finder_info_entry.get(at + 10).copied(),
        ) else {
            return;
        };
        let name_start = at + ATTR_ENTRY_FIXED_LEN;
        let Some(raw_name) = slice(finder_info_entry, name_start, usize::from(name_len)) else {
            return;
        };
        // Each entry ends padded to four bytes of the SIDECAR, not of the entry
        // (copyfile puts the Finder Info at 50, which isn't aligned).
        let Some(next) = (entry_offset + name_start + usize::from(name_len))
            .checked_add(3)
            .map(|end| (end & !3) - entry_offset)
        else {
            return;
        };
        at = next;
        let Some(name) = admissible_name(raw_name) else {
            continue;
        };
        if name == FINDER_INFO_XATTR || name == RESOURCE_FORK_XATTR {
            // Those two have entries of their own; one here is a forgery.
            continue;
        }
        if let Some(value) = slice(bytes, offset as usize, length as usize) {
            push_unique(out, name, value);
        }
    }
}

/// The name an `ATTR` entry spells, if a file may take it: NUL-terminated with
/// no NUL inside, UTF-8, at most `XATTR_MAXNAMELEN` bytes, and neither
/// provenance nor the protected namespace.
fn admissible_name(raw: &[u8]) -> Option<&str> {
    let (last, name) = raw.split_last()?;
    if *last != 0 {
        return None;
    }
    std::str::from_utf8(name)
        .ok()
        .filter(|name| admissible_xattr_name(name))
}

/// Whether an archive may put an xattr named `name` on what it extracts: a name
/// `setxattr(2)` takes as itself (non-empty, no NUL, at most
/// `XATTR_MAXNAMELEN` bytes), and neither provenance nor the protected
/// namespace. Every name read out of an archive passes here.
pub(crate) fn admissible_xattr_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && !name.contains('\0')
        && name != PROVENANCE_XATTR
        && !name.starts_with(SYSTEM_NAMESPACE)
}

fn push_unique(out: &mut Vec<Xattr>, name: &str, value: &[u8]) {
    if out.iter().any(|xattr| xattr.name == name) {
        return;
    }
    out.push(Xattr {
        name: OsString::from(name),
        value: value.to_vec(),
    });
}

/// Writes the sidecar `copyfile(3)` would for `xattrs`: the Finder Info entry
/// (zeros when there's none), the `ATTR` block with every other xattr in the
/// given order, then the resource fork. The caller decides what goes in; a
/// Finder Info that isn't 32 bytes, or a name the block can't spell, is left out.
pub(crate) fn build(xattrs: &[Xattr]) -> Vec<u8> {
    let mut finder_info = [0u8; FINDER_INFO_LEN];
    let mut resource_fork: &[u8] = &[];
    let mut attrs: Vec<(&[u8], &[u8])> = Vec::new();
    for xattr in xattrs {
        let name = xattr.name.as_encoded_bytes();
        if name == FINDER_INFO_XATTR.as_bytes() {
            if let Ok(value) = <[u8; FINDER_INFO_LEN]>::try_from(xattr.value.as_slice()) {
                finder_info = value;
            }
        } else if name == RESOURCE_FORK_XATTR.as_bytes() {
            resource_fork = &xattr.value;
        } else if !name.is_empty() && name.len() <= MAX_NAME_BYTES && !name.contains(&0) {
            attrs.push((name, &xattr.value));
        }
    }
    encode(&finder_info, &attrs, resource_fork)
}

/// The byte layout itself, trusting its input: [`build`] decides what goes in,
/// and the tests hand it names [`build`] would refuse.
fn encode(finder_info: &[u8; FINDER_INFO_LEN], attrs: &[(&[u8], &[u8])], resource_fork: &[u8]) -> Vec<u8> {
    let entries_len: usize = attrs
        .iter()
        .map(|(name, _)| (ATTR_ENTRY_FIXED_LEN + name.len() + 1 + 3) & !3)
        .sum();
    let data_start = ATTR_HEADER_OFFSET + ATTR_HEADER_LEN + entries_len;
    let data_len: usize = attrs.iter().map(|(_, value)| value.len()).sum();
    let total = data_start + data_len;

    let mut out = Vec::with_capacity(total + resource_fork.len());
    out.extend_from_slice(&MAGIC.to_be_bytes());
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(FILLER);
    out.extend_from_slice(&2u16.to_be_bytes());
    for (id, offset, len) in [
        (ENTRY_FINDER_INFO, FINDER_INFO_OFFSET, total - FINDER_INFO_OFFSET),
        (ENTRY_RESOURCE_FORK, total, resource_fork.len()),
    ] {
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&be32(offset));
        out.extend_from_slice(&be32(len));
    }
    out.extend_from_slice(finder_info);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&ATTR_MAGIC.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // debug tag
    out.extend_from_slice(&be32(total));
    out.extend_from_slice(&be32(data_start));
    out.extend_from_slice(&be32(data_len));
    out.extend_from_slice(&[0; 12]); // reserved
    out.extend_from_slice(&0u16.to_be_bytes()); // flags
    out.extend_from_slice(&u16::try_from(attrs.len()).unwrap_or(u16::MAX).to_be_bytes());
    let mut data_at = data_start;
    for (name, value) in attrs {
        out.extend_from_slice(&be32(data_at));
        out.extend_from_slice(&be32(value.len()));
        out.extend_from_slice(&0u16.to_be_bytes());
        out.push(u8::try_from(name.len() + 1).unwrap_or(u8::MAX));
        out.extend_from_slice(name);
        out.push(0);
        out.resize((out.len() + 3) & !3, 0);
        data_at += value.len();
    }
    for (_, value) in attrs {
        out.extend_from_slice(value);
    }
    out.extend_from_slice(resource_fork);
    out
}

/// Whether the xattr `name` with `value` is a reason to write a sidecar at all
/// (David's call: an ordinary zip stays free of `__MACOSX` for Windows users).
/// Provenance and quarantine aren't, and neither is a Finder Info that's all
/// zeros; everything else is. A sidecar that IS written carries quarantine too.
pub(crate) fn worth_keeping(name: &str, value: &[u8]) -> bool {
    match name {
        PROVENANCE_XATTR | QUARANTINE_XATTR => false,
        FINDER_INFO_XATTR => value.iter().any(|byte| *byte != 0),
        _ => true,
    }
}

/// The `__MACOSX` entry that carries the sidecar of the archive item at
/// `inner` (`a/b` → `__MACOSX/a/._b`). Surrounding slashes are ignored.
pub(crate) fn sidecar_entry_name(inner: &str) -> String {
    let inner = inner.trim_matches('/');
    match inner.rsplit_once('/') {
        Some((parent, leaf)) => format!("{MACOSX_FOLDER}/{parent}/._{leaf}"),
        None => format!("{MACOSX_FOLDER}/._{inner}"),
    }
}

/// What a sanitized archive path is to the `__MACOSX` convention.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MacosxEntry {
    /// Not under the root `__MACOSX` folder: an ordinary item.
    Outside,
    /// The sidecar of the item at this inner path.
    Sidecar(String),
    /// The folder itself or anything in it that isn't a sidecar: hidden, unused.
    Other,
}

/// Classifies a SANITIZED inner path (no `..`, no surrounding slashes). Only the
/// archive-root `__MACOSX` counts: that's the only place ditto writes one.
pub(crate) fn classify_macosx(path: &str) -> MacosxEntry {
    let Some(rest) = path.strip_prefix(MACOSX_FOLDER) else {
        return MacosxEntry::Outside;
    };
    let rest = match rest.strip_prefix('/') {
        Some(rest) => rest,
        None if rest.is_empty() => return MacosxEntry::Other,
        None => return MacosxEntry::Outside,
    };
    match sibling_sidecar_target(rest) {
        Some(target) => MacosxEntry::Sidecar(target),
        None => MacosxEntry::Other,
    }
}

/// The sibling a `._name` path shadows (`a/._b` → `a/b`), or `None` when the
/// leaf isn't a sidecar name. The leaf must name something real: `._`, `._.`,
/// and `._..` name nothing a tree can hold.
pub(crate) fn sibling_sidecar_target(path: &str) -> Option<String> {
    let (parent, leaf) = match path.rsplit_once('/') {
        Some((parent, leaf)) => (Some(parent), leaf),
        None => (None, path),
    };
    let target = leaf.strip_prefix("._")?;
    if target.is_empty() || target == "." || target == ".." {
        return None;
    }
    Some(match parent {
        Some(parent) => format!("{parent}/{target}"),
        None => target.to_string(),
    })
}

fn slice(bytes: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    bytes.get(offset..offset.checked_add(len)?)
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = slice(bytes, at, 4)?;
    Some(u32::from_be_bytes(raw.try_into().ok()?))
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = slice(bytes, at, 2)?;
    Some(u16::from_be_bytes(raw.try_into().ok()?))
}

/// A length or offset as the big-endian `u32` the format stores. A sidecar the
/// caller bounded to [`MAX_SIDECAR_BYTES`] always fits.
fn be32(value: usize) -> [u8; 4] {
    u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes()
}

#[cfg(test)]
#[path = "apple_double_test.rs"]
mod tests;
