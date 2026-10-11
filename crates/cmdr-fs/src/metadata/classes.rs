//! Which metadata classes a caller wants: the question a destination asks a
//! source stream (`VolumeReadStream::metadata`), so a source reads only what the
//! destination will keep.

use std::ops::BitOr;

/// A set of the classes [`super::CarriedMetadata`] carries. mtime isn't one:
/// a stream's `modified_at` owns it.
// DEFAULT-OK: the empty set asks for nothing, so a source reads nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataClasses(u8);

impl MetadataClasses {
    /// No class.
    pub const NONE: Self = Self(0);
    /// The creation (birth) date.
    pub const CREATED: Self = Self(1);
    /// The permission bits.
    pub const MODE: Self = Self(1 << 1);
    /// The user flags (Hidden, Locked, append-only, no-dump).
    pub const FLAGS: Self = Self(1 << 2);
    /// The extended attributes.
    pub const XATTRS: Self = Self(1 << 3);
    /// Every class.
    pub const ALL: Self = Self(Self::CREATED.0 | Self::MODE.0 | Self::FLAGS.0 | Self::XATTRS.0);

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every class in `other` is in this set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// This set without the classes in `other`.
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for MetadataClasses {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
