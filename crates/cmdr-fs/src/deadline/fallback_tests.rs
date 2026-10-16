//! `blocking_with_timeout`: the work's value when it ends in time, the caller's
//! fallback when it doesn't.

use super::blocking_with_timeout;
use tokio::time::Duration;

/// How long the two fallback tests' fake blocking work runs. It has to outlast
/// the 50 ms timeout under those tests, and it is ALSO what the whole test costs:
/// the assertion lands at the timeout, then the runtime waits out the blocking
/// thread before the test can end.
const SLOW_CLOSURE: Duration = Duration::from_millis(500);

#[tokio::test]
async fn test_blocking_with_timeout_fast_closure_returns_value() {
    let result = blocking_with_timeout(Duration::from_secs(2), false, || true).await;
    assert!(result);
}

#[tokio::test]
async fn test_blocking_with_timeout_slow_closure_returns_fallback() {
    let result = blocking_with_timeout(Duration::from_millis(50), false, || {
        // allowed-test-sleep: this closure fakes slow blocking work; overrunning the 50 ms
        // timeout is exactly what makes `blocking_with_timeout` return its fallback.
        // 500 ms is a 10x margin over the timeout and `thread::sleep` can never return
        // early, so the only way this flips is the 50 ms timer being 450 ms late.
        std::thread::sleep(SLOW_CLOSURE);
        true
    })
    .await;
    assert!(!result);
}

#[tokio::test]
async fn test_blocking_with_timeout_returns_custom_fallback() {
    let result = blocking_with_timeout(Duration::from_millis(50), 42, || {
        // allowed-test-sleep: this closure fakes slow blocking work; overrunning the 50 ms
        // timeout is what makes `blocking_with_timeout` return the custom fallback
        std::thread::sleep(SLOW_CLOSURE);
        99
    })
    .await;
    assert_eq!(result, 42);
}
