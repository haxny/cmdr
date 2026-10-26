//! Tests for the attachment mapping: a reference crosses as path + kind, never contents.

use super::*;

#[test]
fn an_attachment_maps_to_the_envelope_by_path_and_kind_only() {
    let file = AttachmentRef {
        path: "/photos/one.png".into(),
        kind: AttachmentKindView::File,
    }
    .to_envelope();
    assert_eq!(file.path, "/photos/one.png");
    assert_eq!(file.kind, AttachmentKind::File);

    let folder = AttachmentRef {
        path: "/photos".into(),
        kind: AttachmentKindView::Folder,
    }
    .to_envelope();
    assert_eq!(folder.kind, AttachmentKind::Folder);
}
