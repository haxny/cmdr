//! What `try_open_share` hears back from a real server: one cell per answer the
//! app's `network::share_access` reads by status and command.
//!
//! The `both` fixture is the shape ERR-SHUSC's server had: guests may list its
//! shares, and its `private` share lets only `testuser` in.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::*;
use smb2::types::Command;
use smb2::types::status::NtStatus;

const STEP_TIMEOUT: Duration = Duration::from_secs(5);

fn both(share: &str, username: Option<&str>, password: Option<&str>) -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", share, smb2::testing::both_port(), username, password)
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_refuses_a_guest_at_the_share() {
    // The session comes up (Samba maps the unknown `Guest` to its guest account),
    // and the SHARE turns it away. That's what makes it a sign-in question rather
    // than a missing share.
    match try_open_share(&both("private", None, None), STEP_TIMEOUT).await {
        Err(smb2::Error::Protocol { status, command }) => {
            assert_eq!(status, NtStatus::ACCESS_DENIED);
            assert_eq!(command, Command::TreeConnect);
        }
        other => panic!("a guest on `private` must be refused at TreeConnect, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_lets_the_allowed_account_in() {
    try_open_share(&both("private", Some("testuser"), Some("testpass")), STEP_TIMEOUT)
        .await
        .unwrap_or_else(|e| panic!("`testuser` must be able to open `private`: {e}"));
}

/// The probe reads a real server's GUID with no credentials, from the `auth`
/// fixture too, which refuses a session without them. Two containers answering
/// the SAME GUID would make any same-share test over them prove nothing, so this
/// pins that they differ (each image's Samba mints its own on first start).
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_probe_reads_the_server_guid_without_credentials() {
    let probe = |port| probe_server_guid("127.0.0.1", port, STEP_TIMEOUT);
    let guest = probe(smb2::testing::guest_port())
        .await
        .expect("the guest fixture answers");
    let auth = probe(smb2::testing::auth_port())
        .await
        .expect("the auth fixture answers");

    assert_ne!(guest, smb2::pack::Guid::ZERO, "Samba answers a real GUID");
    assert_ne!(auth, smb2::pack::Guid::ZERO, "Samba answers a real GUID");
    assert_ne!(guest, auth, "two fixture containers must be two servers");
    let again = probe(smb2::testing::guest_port()).await.expect("answers again");
    assert_eq!(guest, again, "a server's GUID is stable across connections");
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_share_names_a_missing_share() {
    match try_open_share(&both("no-such-share", None, None), STEP_TIMEOUT).await {
        Err(smb2::Error::Protocol { status, command }) => {
            assert_eq!(status, NtStatus::BAD_NETWORK_NAME);
            assert_eq!(command, Command::TreeConnect);
        }
        other => panic!("a share the server doesn't have must answer bad network name, got {other:?}"),
    }
}
