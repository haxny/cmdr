use cmdr_fs::volume::ScanStopSignal;

use crate::SyncControl;

#[tokio::test]
async fn stop_outranks_pause() {
    let control = SyncControl::new();
    control.set_paused(true);
    control.cancel();
    assert!(control.stop_or_park().await);
}

#[tokio::test]
async fn a_pause_holds_until_released_and_cancel_ends_it() {
    let control = SyncControl::new();
    assert!(!control.is_stopping_or_paused());
    control.set_paused(true);
    assert!(control.is_stopping_or_paused());
    let waiter = tokio::spawn({
        let control = control.clone();
        async move { control.stop_or_park().await }
    });
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    control.set_paused(false);
    assert!(!waiter.await.unwrap());

    control.set_paused(true);
    let waiter = tokio::spawn({
        let control = control.clone();
        async move { control.stop_or_park().await }
    });
    tokio::task::yield_now().await;
    control.cancel();
    assert!(waiter.await.unwrap());
}
