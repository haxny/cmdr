//! The identities a line names, kept to scrub bare repeats of them from the rest of it. Both
//! the span pass (`markers.rs`) and the external-text pass (`detail.rs`) collect them, so they
//! live in a leaf neither has to reach through the other for.

use super::tokens::is_redacted_segment;

/// Shortest keyed value worth scrubbing from prose by its spelling. Shorter ones (`tv`, `pi`)
/// are too likely to be part of an ordinary word.
const MIN_ECHOED_IDENTITY_CHARS: usize = 3;

/// A raw identity the line names, paired with the token it gets there.
#[derive(Clone)]
pub(super) struct EchoedIdentity {
    raw: String,
    token: String,
    /// A path's last segment: the one kind of identity scrubbed from plain prose too
    /// (`detail::scrub_leaf_echoes`), since macOS repeats a file's name in its own error text.
    is_leaf: bool,
}

impl EchoedIdentity {
    /// An identity worth scrubbing by its spelling, or `None` for one that isn't: `None`, too
    /// short to tell from part of a word, already a token, or holding a char a line can only
    /// carry as a marker.
    pub(super) fn new(raw: String, token: String, is_leaf: bool) -> Option<Self> {
        let scrubbable = raw != "None"
            && raw.chars().count() >= MIN_ECHOED_IDENTITY_CHARS
            && !is_redacted_segment(&raw)
            && !raw.chars().any(cmdr_fs::log_private::is_reserved);
        scrubbable.then_some(Self { raw, token, is_leaf })
    }

    /// The identity as the line spells it.
    pub(super) fn raw(&self) -> &str {
        &self.raw
    }

    /// The token that replaces it.
    pub(super) fn token(&self) -> &str {
        &self.token
    }

    /// Whether it's a path's last segment.
    pub(super) fn is_leaf(&self) -> bool {
        self.is_leaf
    }
}

/// Longest first, so a share named `Plans` can't split `Client Plans` before it's seen.
pub(super) fn sort_longest_first(identities: &mut [EchoedIdentity]) {
    identities.sort_by_key(|identity| std::cmp::Reverse(identity.raw.len()));
}
