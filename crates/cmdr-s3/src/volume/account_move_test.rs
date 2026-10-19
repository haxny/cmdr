//! An "Other S3-compatible" server moving to a new address keeps its
//! unfinished writes on record (#545): the sweep at the new address aborts an
//! upload recorded at the old one, and settles a cut-off PUT the same way.
//! Without the move, the record stays keyed by the old host and port, which
//! no connect asks about again, and the parts stay billed.
//!
//! Over `fake_s3.rs`, which answers any host name: the two volumes are one
//! server under two addresses, as a move is.

use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use url::Url;

use super::S3Volume;
use super::fake_s3::{BUCKET, FakeS3};
use super::follow_account_move;
use super::upload_ledger::{UnfinishedUpload, UnsettledPut};
use crate::params::S3Provider;

/// The fake as an "Other" server at `host`. Each cell names hosts of its own:
/// the ledger is process-wide.
fn at(s3: &FakeS3, host: &str) -> S3Volume {
    s3.volume_for(S3Provider::Other {
        endpoint: Url::parse(&format!("http://{host}")).expect("a valid URL"),
        region: None,
        path_style: true,
    })
}

/// An upload a crash left open on the server and on record at `volume`'s
/// address.
fn leave_open_upload(s3: &FakeS3, volume: &S3Volume, key: &str) -> UnfinishedUpload {
    let upload = UnfinishedUpload {
        account: volume.inner.account(),
        bucket: BUCKET.to_string(),
        key: key.to_string(),
        upload_id: s3.open_upload(key),
    };
    volume.inner.ledger.started(&upload);
    volume.inner.ledger.abandoned(&upload);
    upload
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sweep_at_the_new_address_aborts_an_upload_recorded_at_the_old_one() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let old = at(&s3, "nas-old.example");
    let upload = leave_open_upload(&s3, &old, "big.mov");
    let new = at(&s3, "nas-new.example");

    let moved = follow_account_move(&VolumeHost::detached(), &old.inner.params(), &new.inner.params());

    assert_eq!(moved, 1, "one record follows the move");
    assert_eq!(new.sweep_unfinished_uploads().await, 1, "the new address aborts it");
    assert_eq!(s3.open_uploads(), 0, "nothing stays billed");
    assert!(
        old.inner.ledger.leftovers(&upload.account).is_empty(),
        "nothing stays on record at the old address"
    );
    assert!(new.inner.ledger.leftovers(&new.inner.account()).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cut_off_put_follows_the_move_too() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let old = at(&s3, "put-old.example");
    s3.seed_with_meta("cut.bin", 10, &["x-amz-meta-cmdr-write: token-old"]);
    let put = UnsettledPut {
        account: old.inner.account(),
        bucket: BUCKET.to_string(),
        key: "cut.bin".to_string(),
        token: "token-old".to_string(),
        size: 4_096,
    };
    old.inner.ledger.started(&put);
    old.inner.ledger.abandoned(&put);
    let new = at(&s3, "put-new.example");

    follow_account_move(&VolumeHost::detached(), &old.inner.params(), &new.inner.params());
    new.sweep_unfinished_uploads().await;

    assert!(s3.object("cut.bin").is_none(), "the truncated body is gone");
    assert!(new.inner.ledger.unsettled_puts(&new.inner.account()).is_empty());
}

/// ❌ Another account's records stay where they are: a move carries only the
/// moving account's.
#[tokio::test(flavor = "multi_thread")]
async fn a_move_leaves_another_accounts_records_alone() {
    let s3 = FakeS3::start(Duration::ZERO).await;
    let other = at(&s3, "other-stays.example");
    let theirs = leave_open_upload(&s3, &other, "theirs.bin");
    let old = at(&s3, "mover-old.example");
    let new = at(&s3, "mover-new.example");

    let moved = follow_account_move(&VolumeHost::detached(), &old.inner.params(), &new.inner.params());

    assert_eq!(moved, 0);
    assert_eq!(other.inner.ledger.leftovers(&theirs.account), vec![theirs]);
    assert!(new.inner.ledger.leftovers(&new.inner.account()).is_empty());
}
