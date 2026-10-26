//! Dialing a place the user saved: one flight per place, then the per-protocol dial.
//!
//! Every dial of a saved place comes through [`connect_saved_place_shared`]: the
//! `connect_saved_place` command (the frontend's connect flow and sign-in sheet) and
//! Ask Cmdr's `connect_volume`. A saved place registers its volume only once its dial
//! lands, so the "already registered?" refusal can't guard the 30 s a dial is out;
//! the flight (`network::dial_flights`) can.

use std::sync::LazyLock;

use cmdr_sftp::SftpConnectionParams;
use tokio_util::sync::CancellationToken;

use super::outcomes::{SavedEntry, outcome_from_s3, outcome_from_sftp, outcome_from_webdav, saved_by_id};
use super::{SavedPlaceRefusal, ServerConnectOutcome, cancel_backend_attempts, nfs_places, webdav_params};
use crate::network::dial_flights::{DialFlights, Share};
use crate::network::one_shot_credentials::SecretOffer;
use crate::network::{s3_volume_wiring, sftp_volume_wiring, smb_saved_shares, webdav_volume_wiring};

/// What one dial of a saved place answers.
pub(crate) type SavedPlaceDial = Result<ServerConnectOutcome, SavedPlaceRefusal>;

/// The saved-place dials out right now, by place.
static SAVED_PLACE_DIALS: LazyLock<DialFlights<SavedPlaceDial>> = LazyLock::new(DialFlights::new);

/// `connect_saved_place`'s body, for in-process callers too, so every dial of a saved
/// place joins the one flight.
///
/// A caller holding nothing but stored credentials shares a dial already out; one
/// carrying a typed secret or account dials its own, after it. `attempt_id` calls off
/// only this caller (`cancel_server_connect`), as does dropping the future; the dial
/// itself stops once nobody waits on it.
pub(crate) async fn connect_saved_place_shared(
    volume_id: &str,
    attempt_id: &str,
    secret: Option<SecretOffer>,
    username: Option<String>,
) -> SavedPlaceDial {
    let share = if secret.is_none() && username.is_none() {
        Share::Plain
    } else {
        Share::Never
    };
    let place = volume_id.to_string();
    let dial = move |call_off: CancellationToken| dial_saved_place(place, secret, username, call_off);
    SAVED_PLACE_DIALS
        .dial(volume_id, attempt_id, share, dial)
        .await
        .unwrap_or(Ok(ServerConnectOutcome::Cancelled))
}

/// Calls off the saved-place caller `attempt_id`, answering whether one was waiting.
pub(super) fn cancel(attempt_id: &str) -> bool {
    SAVED_PLACE_DIALS.cancel(attempt_id)
}

/// One dial of a saved place, under an attempt id of its own: callers come and go,
/// and only the flight calls this off ([`DialFlights`]).
async fn dial_saved_place(
    volume_id: String,
    secret: Option<SecretOffer>,
    username: Option<String>,
    call_off: CancellationToken,
) -> SavedPlaceDial {
    let attempt_id = format!("saved-place-dial-{}", uuid::Uuid::new_v4());
    let dial = dial_saved_place_now(volume_id, &attempt_id, secret, username);
    tokio::pin!(dial);
    let outcome = tokio::select! {
        biased;
        outcome = &mut dial => outcome,
        () = call_off.cancelled() => {
            // ❗ Marked ahead as well as cancelled: an SFTP or S3 dial reads the
            // Keychain before it files its attempt, and a plain cancel would miss it.
            crate::network::connect_wiring::call_off_early(&attempt_id);
            cancel_backend_attempts(&attempt_id);
            // A cancel comes back FROM the dial, so it lands as `Cancelled` and
            // registers nothing.
            dial.await
        }
    };
    crate::network::connect_wiring::forget_call_off(&attempt_id);
    outcome
}

/// The dial itself, by the saved entry's protocol.
async fn dial_saved_place_now(
    volume_id: String,
    attempt_id: &str,
    secret: Option<SecretOffer>,
    username: Option<String>,
) -> SavedPlaceDial {
    if crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .is_some()
    {
        return Err(SavedPlaceRefusal::AlreadyConnected { volume_id });
    }
    // ❗ Before the store read: a move landing between the read and the dial's
    // landing must still refuse it, or a dial to the old address (a host-key
    // approval answered after the move) saves it again (`connect_wiring::DialTicket`).
    let set_out = crate::network::connect_wiring::DialTicket::now();
    let Some(saved) = saved_by_id(&volume_id) else {
        return Err(SavedPlaceRefusal::NoSuchServer { volume_id });
    };
    Ok(match saved {
        SavedEntry::Sftp(entry) => {
            let mut params = SftpConnectionParams::new(&entry.host, entry.port, &entry.username, entry.remote_root);
            params.key_file = entry.key_file.map(std::path::PathBuf::from);
            params.use_agent = entry.use_agent;
            params.auto_reconnect = entry.auto_reconnect;
            outcome_from_sftp(
                sftp_volume_wiring::connect_and_register_since(
                    set_out,
                    &entry.display_name,
                    entry.start_folder,
                    params,
                    attempt_id,
                    secret,
                )
                .await,
            )
        }
        SavedEntry::Webdav(entry) => {
            let Some(mut params) = webdav_params(&entry.url, &entry.username, &entry.remote_root) else {
                return Ok(ServerConnectOutcome::InvalidUrl);
            };
            params.auto_reconnect = entry.auto_reconnect;
            outcome_from_webdav(
                webdav_volume_wiring::connect_and_register_since(
                    set_out,
                    &entry.display_name,
                    entry.start_folder,
                    params,
                    attempt_id,
                    secret,
                )
                .await,
            )
        }
        SavedEntry::S3(entry) => outcome_from_s3(
            s3_volume_wiring::connect_and_register_since(
                set_out, // Nothing typed: the account keeps the name it has.
                "", entry, attempt_id, secret,
            )
            .await,
        ),
        SavedEntry::Nfs(entry) => nfs_places::connect_saved(set_out, entry, attempt_id).await,
        // A saved SMB share: mounted as its account, then connected the usual way.
        SavedEntry::Smb(row) => smb_saved_shares::connect_saved_share(row, set_out, attempt_id, secret, username).await,
    })
}
