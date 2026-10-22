//! What the add form asks a server before a place exists: its exports, and who
//! the files in one belong to.

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::dial::{self, DialOptions};
use crate::errors::NfsConnectError;
use crate::params::{NfsIdentity, NfsServer};
use crate::proto::nfs3::ListingEvent;

/// Who the add form should act as on an export, and why that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NfsIdentityGuess {
    /// The uid and gid to fill the form with.
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
/// as `this_mac`: what the add form fills "Act as user ID" and "Group ID"
/// with. Answers `this_mac` itself ([`NfsIdentitySource::ThisMac`]) when nothing
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
    let mounted = dial::mount(server, export, this_mac, &options, &cancel).await?;
    let mut owners: Vec<NfsIdentity> = Vec::new();
    let listing = mounted.nfs.list_dir(&mounted.root, 64 * 1024, |event| match event {
        ListingEvent::Entries(batch) => owners.extend(batch.iter().map(|entry| NfsIdentity {
            uid: entry.attributes.uid,
            gid: entry.attributes.gid,
        })),
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
    let root_owner = NfsIdentity {
        uid: mounted.root_attributes.uid,
        gid: mounted.root_attributes.gid,
    };
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
            identity: NfsIdentity { uid, gid },
            source: NfsIdentitySource::MostCommonOwner,
        };
    }
    if root_owner.uid != 0 {
        return NfsIdentityGuess {
            identity: root_owner,
            source: NfsIdentitySource::RootOwner,
        };
    }
    NfsIdentityGuess {
        identity: this_mac,
        source: NfsIdentitySource::ThisMac,
    }
}

#[cfg(test)]
#[path = "discovery_test.rs"]
mod discovery_test;
