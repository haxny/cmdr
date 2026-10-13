use std::time::{Duration, SystemTime};

use objc2_foundation::{NSException, NSString};

use super::*;

fn exception(name: &str, reason: &str) -> objc2::rc::Retained<NSException> {
    // SAFETY: a nil `userInfo` dictionary, so there's no element type to get wrong.
    unsafe {
        NSException::exceptionWithName_reason_userInfo(
            &NSString::from_str(name),
            Some(&NSString::from_str(reason)),
            None,
        )
    }
}

#[test]
fn the_line_names_the_exception_and_its_reason() {
    let e = exception("NSInvalidArgumentException", "fileType 'x' is not a valid UTI");
    assert_eq!(
        exception_line(&e),
        "NSInvalidArgumentException: fileType 'x' is not a valid UTI"
    );
}

#[test]
fn an_overlong_reason_is_capped_on_a_char_boundary() {
    let e = exception("NSGenericException", &"é".repeat(MAX_LINE_BYTES));
    let line = exception_line(&e);
    assert!(line.len() <= MAX_LINE_BYTES);
    assert!(line.starts_with("NSGenericException: é"));
}

#[test]
fn a_recorded_exception_is_read_back_for_the_matching_crash() {
    let dir = crate::test_support::TestDir::new("objc-exception-roundtrip");
    let sidecar = dir.join(FILE_NAME);
    record(&exception("NSRangeException", "index 3 beyond bounds"), &sidecar);

    assert_eq!(
        line_near(&sidecar, SystemTime::now()).as_deref(),
        Some("NSRangeException: index 3 beyond bounds")
    );
}

#[test]
fn a_sidecar_from_another_crash_is_ignored() {
    let dir = crate::test_support::TestDir::new("objc-exception-stale");
    let sidecar = dir.join(FILE_NAME);
    record(&exception("NSRangeException", "old"), &sidecar);

    let an_hour_later = SystemTime::now() + Duration::from_secs(3_600);
    assert_eq!(line_near(&sidecar, an_hour_later), None);
}

#[test]
fn no_sidecar_means_no_line() {
    let dir = crate::test_support::TestDir::new("objc-exception-missing");
    assert_eq!(line_near(&dir.join(FILE_NAME), SystemTime::now()), None);
}
