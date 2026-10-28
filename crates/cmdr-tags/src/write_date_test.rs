//! `date_is_writable`: the check the editor flags a Date with is the writer's own.

use crate::{AudioFormat, date_is_writable};

#[test]
fn date_is_writable_agrees_with_what_the_writer_refuses() {
    assert!(date_is_writable(AudioFormat::Mp3, "2010"));
    assert!(date_is_writable(AudioFormat::Mp3, "2010-09-20"));
    assert!(!date_is_writable(AudioFormat::Mp3, "2010-20-09"));
    assert!(!date_is_writable(AudioFormat::Mp3, "kolem roku"));
    // Other formats keep the date as text: the writer never refuses it.
    assert!(date_is_writable(AudioFormat::Mp4, "2010-20-09"));
}
