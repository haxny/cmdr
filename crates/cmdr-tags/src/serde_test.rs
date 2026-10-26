//! The JSON shapes the app's IPC will carry (Plan 2): camelCase like the model types, and
//! every value round-trips.

use std::io::ErrorKind;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::{ContentKind, TagError};
use crate::tools::CaseTool;
use crate::undo::{ApplyId, RestoreReport};
use crate::write::WriteOptions;

/// `value` as JSON, checked to read back as itself.
fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) -> Value {
    let json = serde_json::to_value(value).unwrap();
    let back: T = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(&back, value);
    json
}

#[test]
fn write_options_are_camel_case_and_default_to_lossless_music() {
    assert_eq!(
        WriteOptions::default(),
        WriteOptions {
            kind: ContentKind::Music,
            track_total: None,
            disc_total: None,
            allow_frame_loss: false,
        }
    );
    let opts = WriteOptions {
        kind: ContentKind::Audiobook,
        track_total: Some(12),
        disc_total: None,
        allow_frame_loss: true,
    };
    assert_eq!(
        round_trip(&opts),
        json!({"kind": "audiobook", "trackTotal": 12, "discTotal": null, "allowFrameLoss": true})
    );
}

#[test]
fn tag_errors_are_tagged_and_io_kinds_are_strings() {
    let cases = [
        (TagError::NotSupported, json!({"type": "notSupported"})),
        (
            TagError::Io(ErrorKind::NotFound, "gone".to_owned()),
            json!({"type": "io", "data": ["NotFound", "gone"]}),
        ),
        (
            TagError::Io(ErrorKind::PermissionDenied, "no".to_owned()),
            json!({"type": "io", "data": ["PermissionDenied", "no"]}),
        ),
        (
            TagError::Parse("bad".to_owned()),
            json!({"type": "parse", "data": "bad"}),
        ),
        (
            TagError::WouldDrop(vec!["TXXX:Comment".to_owned()]),
            json!({"type": "wouldDrop", "data": ["TXXX:Comment"]}),
        ),
        (
            TagError::UnwritableExtra("PERFORMER".to_owned()),
            json!({"type": "unwritableExtra", "data": "PERFORMER"}),
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(round_trip(&error), expected);
    }
}

#[test]
fn an_unknown_io_kind_reads_as_other() {
    let error: TagError = serde_json::from_value(json!({"type": "io", "data": ["SomethingNew", "x"]})).unwrap();
    assert_eq!(error, TagError::Io(ErrorKind::Other, "x".to_owned()));
}

#[test]
fn restore_report_apply_id_and_case_tool_round_trip() {
    let report = RestoreReport {
        restored: vec![PathBuf::from("/a.mp3")],
        missing: vec![PathBuf::from("/b.mp3")],
        failed: vec![(PathBuf::from("/c.mp3"), TagError::Write("x".to_owned()))],
    };
    assert_eq!(
        round_trip(&report),
        json!({
            "restored": ["/a.mp3"],
            "missing": ["/b.mp3"],
            "failed": [["/c.mp3", {"type": "write", "data": "x"}]],
        })
    );
    assert_eq!(round_trip(&ApplyId("op-1".to_owned())), json!("op-1"));
    assert_eq!(round_trip(&CaseTool::Sentence), json!("sentence"));
    assert_eq!(round_trip(&CaseTool::Upper), json!("upper"));
}

#[test]
fn a_cover_is_a_byte_array_on_the_wire() {
    let cover = crate::model::Cover {
        mime: "image/png".to_owned(),
        data: vec![1, 2, 255].into(),
    };
    assert_eq!(round_trip(&cover), json!({"mime": "image/png", "data": [1, 2, 255]}));
}
