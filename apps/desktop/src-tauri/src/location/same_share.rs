//! One share reached by two addresses is ONE share: which volumes are proven the
//! same, and which of them may answer a read addressed to another.
//!
//! A NAS mounted over its LAN IP and over Tailscale is two volumes with two ids,
//! two indexes, and two importance stores. Two rules, both proof-based
//! (`agent/tools/DETAILS.md` § The location contract):
//!
//! - **Same share** (`sameShareAs`): both record the same server GUID (non-zero:
//!   [`ServerGuid`] can't hold the all-zero one) AND the same share name, ignoring
//!   case. The GUID names the BOX, so it never groups alone. ❌ No fuzzy "possibly
//!   the same": an unproven pair isn't grouped.
//! - **Redirectable**: the same share, signed in as the same account. Another
//!   account can see different files on the same share, so a different-username
//!   sibling stays a `sameShareAs` hint and never answers for the volume asked.
//!
//! Pure over [`VolumeFacts`]; `facts.rs` reads each share's identity off the
//! saved-share store (`serverGuid`) and the live sessions (`network::smb_server_guid`).

use crate::network::server_guid::ServerGuid;
use cmdr_fs::volume::app_paths::rebase;
use serde::Serialize;

use super::Location;
use super::resolve::{ResolvedLocation, VolumeFact, VolumeFacts};

/// What proves which share a volume is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShareIdentity {
    /// `None` until a session or the probe learned it.
    pub server_guid: Option<ServerGuid>,
    /// Case- and NFC-folded, so `naspi` and `NASPI` are one share.
    share_name: String,
    /// The account the share is opened as, folded; `None` for a guest.
    username: Option<String>,
}

impl ShareIdentity {
    /// An identity from the stored fields: a GUID that doesn't parse (malformed or
    /// all zeros) is no GUID, and a `Guest` sign-in is no account.
    pub(crate) fn new(server_guid: Option<&str>, share_name: &str, username: Option<&str>) -> Self {
        Self {
            server_guid: server_guid.and_then(ServerGuid::parse),
            share_name: fold(share_name),
            username: username
                .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("guest"))
                .map(fold),
        }
    }

    /// Proven the same share: one server GUID, one share name.
    pub(crate) fn same_share(&self, other: &Self) -> bool {
        matches!((&self.server_guid, &other.server_guid), (Some(a), Some(b)) if a == b)
            && self.share_name == other.share_name
    }

    /// The same share, opened as the same account: a read on one may be answered
    /// by the other.
    pub(crate) fn redirectable(&self, other: &Self) -> bool {
        self.same_share(other) && self.username == other.username
    }
}

/// The volume a read or a row was asked about, when another member of its share
/// answered in its place: `{ "volumeId": … }` beside the answer's `location`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Via {
    pub volume_id: String,
}

fn fold(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    name.nfc().flat_map(char::to_lowercase).collect()
}

/// How well a share member can answer a read, best first. A folder-independent
/// order; `list_dir` refines it per folder (`read/listing.rs`).
fn standing(volume: &VolumeFact) -> (u8, u64) {
    match (volume.live, volume.index.live, volume.index.stored) {
        (true, true, _) => (3, 0),
        (true, false, _) => (2, 0),
        (false, _, true) => (1, volume.index.stored_as_of.unwrap_or(0)),
        (false, _, false) => (0, 0),
    }
}

impl VolumeFacts {
    /// Every other volume proven to be the same share as `id`, in listing order.
    pub(crate) fn same_share_as(&self, id: &str) -> Vec<String> {
        let Some(identity) = self.get(id).and_then(|v| v.share.as_ref()) else {
            return Vec::new();
        };
        self.volumes
            .iter()
            .filter(|v| v.id != id && v.share.as_ref().is_some_and(|other| identity.same_share(other)))
            .map(|v| v.id.clone())
            .collect()
    }

    /// The members of `id`'s share that may answer a read on it, `id` first:
    /// itself, then every same-share, same-account sibling.
    fn redirect_group(&self, id: &str) -> Vec<&VolumeFact> {
        let Some(asked) = self.get(id) else {
            return Vec::new();
        };
        let siblings = self
            .volumes
            .iter()
            .filter(|v| v.id != id && matches!((&asked.share, &v.share), (Some(a), Some(b)) if a.redirectable(b)));
        std::iter::once(asked).chain(siblings).collect()
    }

    /// `resolved` re-addressed onto every member that may answer for it, the
    /// volume asked first. Each sibling's location is the same place under its own
    /// root, with `via` naming the volume asked.
    pub(crate) fn readers(&self, resolved: &ResolvedLocation) -> Vec<ResolvedLocation> {
        let asked = &resolved.location.volume_id;
        let mut readers = vec![resolved.clone()];
        for member in self.redirect_group(asked).into_iter().skip(1) {
            let Some(path) = rebase(&resolved.location.path, &resolved.root, &member.root) else {
                continue;
            };
            readers.push(ResolvedLocation {
                location: Location {
                    volume_id: member.id.clone(),
                    path,
                },
                name: member.name.clone(),
                kind: member.kind,
                root: member.root.clone(),
                live: member.live,
                index: member.index.clone(),
                rebased_from: resolved.rebased_from.clone(),
                via: Some(asked.clone()),
            });
        }
        readers
    }

    /// The member of `id`'s share a place on it is best addressed on: live with an
    /// index, then live, then the freshest index on disk, `id` itself on a tie (or
    /// when it has no redirectable sibling). What `important_folders` re-addresses
    /// a member's rows onto.
    pub(crate) fn preferred_member(&self, id: &str) -> Option<&VolumeFact> {
        self.redirect_group(id)
            .into_iter()
            .enumerate()
            .max_by_key(|(order, member)| (standing(member), std::cmp::Reverse(*order)))
            .map(|(_, member)| member)
    }

    /// `id` and its redirectable siblings, the scope a read naming `id` spans.
    pub(crate) fn redirect_group_ids(&self, id: &str) -> Vec<String> {
        self.redirect_group(id).into_iter().map(|v| v.id.clone()).collect()
    }
}

#[cfg(test)]
#[path = "same_share_tests.rs"]
mod tests;
