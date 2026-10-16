//! The parser contract: what every format's `parse` (in [`super::zip`] /
//! [`super::tar`] / [`super::sevenz`]) hands the shared tree builder in
//! [`super::index`], and the entry-count cap a forward-scanning parser enforces
//! before the tree ever sees its output. A leaf: the parsers and the index both
//! build on it, and it builds on neither.

use cmdr_fs::metadata::Xattr;

/// Maximum number of nodes (files plus synthesized directories) in one archive's
/// tree. The backstop against the many-entries axis of memory amplification: the
/// per-entry `MAX_COMPONENT_DEPTH` cap (in `name.rs`) bounds each entry's cost,
/// but a central directory with a huge number of deep-ish
/// entries could still sum to an oversized tree from a modest input. Exceeding
/// this fails the whole parse with a typed error rather than risking an OOM.
///
/// 2,000,000 is well beyond real archives (the Linux kernel source is ~90k
/// files, a Chromium checkout ~400k), so a legitimate archive never trips it; it
/// only fires on a hostile blow-up. It bounds node *count*; per-node path length
/// is separately bounded by the zip name field (`u16`, 64 KB), so worst-case
/// tree memory is bounded too.
pub(super) const MAX_TREE_NODES: usize = 2_000_000;

/// A format-neutral parsed entry: the fields every format's parser produces for
/// the shared tree builder. The per-format read handle (`H` in
/// `index::build_index`) travels alongside it, not inside.
pub(super) struct RawEntry {
    /// Raw, attacker-controlled entry name (pre-sanitization). `\` and `..` and
    /// absolute paths are handled by `sanitize_entry_name`, identically for
    /// every format — the Zip Slip defense is format-independent.
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// Uncompressed size (0 for a dir/symlink is fine — dirs report `None`).
    pub size: u64,
    pub compressed_size: u64,
    pub modified: Option<i64>,
    pub encrypted: bool,
    /// The POSIX permission bits the archive recorded, `None` when it recorded
    /// none. Each format's parser reads its own carrier (zip external
    /// attributes, the tar header, the 7z unix extension) and every one of them
    /// answers `None` rather than a plausible default.
    pub mode: Option<u32>,
    /// The xattrs the archive recorded on the entry itself (a tar's
    /// `SCHILY.xattr.*` pax records), already filtered to names a file may take.
    pub xattrs: Vec<Xattr>,
    /// A tar `._name` entry's bytes, read during the one forward scan because
    /// the scan can't come back for them: if it pairs with a sibling, they're
    /// that sibling's AppleDouble sidecar. `None` for every other entry, and
    /// for one past [`crate::apple_double::MAX_SIDECAR_BYTES`].
    pub sidecar_bytes: Option<Vec<u8>>,
}
