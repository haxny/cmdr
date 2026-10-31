//! What the add form asks a server before a place exists: its exports, and who
//! the files in one belong to.

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::dial::{self, DialOptions};
use crate::errors::NfsConnectError;
use crate::params::{NfsIdentity, NfsServer};
use crate::proto::nfs3::ListingEvent;
use crate::proto::rpc::MAX_GIDS;

/// Who the add form should act as on an export, and why that one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsIdentityGuess {
    /// The uid, gid, and other groups to fill the form with.
    pub identity: NfsIdentity,
    /// Where they came from, so the form can say.
    pub source: NfsIdentitySource,
}

/// Where an [`NfsIdentityGuess`] came from, best evidence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfsIdentitySource {
    /// The most common non-zero owner among the export root's entries.
    MostCommonOwner,
    /// No entry had a non-zero owner, but the export root itself does.
    RootOwner,
    /// Everything there is owned by uid 0, or there's nothing there: the
    /// caller's own fallback, this Mac's uid and gid.
    ThisMac,
}

/// The export paths `server` lists (MOUNT's `EXPORT`), each canonical
/// (`cmdr_fs::volume::canonical_nfs_export`). A name that isn't UTF-8 is left
/// out.
///
/// ❗ A server may refuse to list ([`NfsConnectError::ListingRefused`]) and still
/// mount an export typed by hand, so a refusal here is ❌ never a reason to
/// refuse the place.
pub async fn list_exports(server: &NfsServer, cancel: CancellationToken) -> Result<Vec<String>, NfsConnectError> {
    dial::list_exports(server, &DialOptions::PRODUCTION, &cancel).await
}

/// Who owns the files in `export`, from one `MNT` and one listing of its root
/// as `this_mac`: what the add form fills "User ID", "Group ID", and "Other
/// group IDs" with. Answers `this_mac` itself ([`NfsIdentitySource::ThisMac`]) when nothing
/// there has a non-zero owner.
///
/// ❗ A squashed probe may be refused the listing (a `700` root owned by
/// someone else); then the root's own owner, from the mount's `GETATTR`, is
/// the answer.
pub async fn guess_identity(
    server: &NfsServer,
    export: &str,
    this_mac: NfsIdentity,
    cancel: CancellationToken,
) -> Result<NfsIdentityGuess, NfsConnectError> {
    let options = DialOptions::PRODUCTION;
    let mounted = dial::mount(server, export, &this_mac, &options, &cancel).await?;
    let mut owners: Vec<NfsIdentity> = Vec::new();
    let listing = mounted.nfs.list_dir(&mounted.root, 64 * 1024, |event| match event {
        ListingEvent::Entries(batch) => owners.extend(
            batch
                .iter()
                .map(|entry| NfsIdentity::new(entry.attributes.uid, entry.attributes.gid)),
        ),
        ListingEvent::Restarted => owners.clear(),
    });
    let listed = tokio::select! {
        biased;
        () = cancel.cancelled() => {
            mounted.nfs.connection().close();
            return Err(NfsConnectError::Cancelled);
        }
        listed = tokio::time::timeout(options.budget, listing) => listed,
    };
    mounted.nfs.connection().close();
    if !matches!(listed, Ok(Ok(_))) {
        owners.clear();
    }
    let root_owner = NfsIdentity::new(mounted.root_attributes.uid, mounted.root_attributes.gid);
    dial::unmount(mounted.addresses.mount, &mounted.export, mounted.credential, &options).await;
    Ok(pick_identity(&owners, root_owner, this_mac))
}

/// The guess from what the root's entries and the root itself are owned by.
/// Ties go to the lower uid, so the answer doesn't move between two calls.
pub(super) fn pick_identity(
    owners: &[NfsIdentity],
    root_owner: NfsIdentity,
    this_mac: NfsIdentity,
) -> NfsIdentityGuess {
    let seen = groups_seen(owners, root_owner.gid);
    let mut counts: HashMap<u32, (usize, HashMap<u32, usize>)> = HashMap::new();
    for owner in owners.iter().filter(|owner| owner.uid != 0) {
        let (count, gids) = counts.entry(owner.uid).or_default();
        *count += 1;
        *gids.entry(owner.gid).or_default() += 1;
    }
    let best = counts
        .into_iter()
        .max_by(|(uid_a, (a, _)), (uid_b, (b, _))| a.cmp(b).then(uid_b.cmp(uid_a)));
    if let Some((uid, (_, gids))) = best {
        let gid = gids
            .into_iter()
            .max_by(|(gid_a, a), (gid_b, b)| a.cmp(b).then(gid_b.cmp(gid_a)))
            .map_or(uid, |(gid, _)| gid);
        return NfsIdentityGuess {
            identity: with_other_groups(NfsIdentity::new(uid, gid), &seen),
            source: NfsIdentitySource::MostCommonOwner,
        };
    }
    if root_owner.uid != 0 {
        return NfsIdentityGuess {
            identity: with_other_groups(root_owner, &seen),
            source: NfsIdentitySource::RootOwner,
        };
    }
    NfsIdentityGuess {
        identity: with_other_groups(this_mac, &seen),
        source: NfsIdentitySource::ThisMac,
    }
}

/// Every non-zero group on the root's entries and the root itself, most common
/// first, ties to the lower gid.
///
/// ❗ Someone else's file counts: a group on it is exactly how a person reads
/// it, and `AUTH_SYS` proves nothing either way, so this claims no more than
/// guessing the owner's uid already does. ❌ Never group 0, the same as uid 0.
fn groups_seen(owners: &[NfsIdentity], root_gid: u32) -> Vec<u32> {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for gid in owners.iter().map(|owner| owner.gid).chain([root_gid]) {
        if gid != 0 {
            *counts.entry(gid).or_default() += 1;
        }
    }
    let mut seen: Vec<(u32, usize)> = counts.into_iter().collect();
    seen.sort_unstable_by(|(gid_a, a), (gid_b, b)| b.cmp(a).then(gid_a.cmp(gid_b)));
    seen.into_iter().map(|(gid, _)| gid).collect()
}

/// `identity` with the groups `seen` first, then whichever others it came with
/// (only this Mac's has any), minus its primary group, up to the RFC cap.
fn with_other_groups(mut identity: NfsIdentity, seen: &[u32]) -> NfsIdentity {
    let own = std::mem::take(&mut identity.other_gids);
    for gid in seen.iter().copied().chain(own) {
        if identity.other_gids.len() < MAX_GIDS && gid != identity.gid && !identity.other_gids.contains(&gid) {
            identity.other_gids.push(gid);
        }
    }
    identity
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod discovery_test;
