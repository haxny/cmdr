//! What the dialog hears back from a server card's approve.

use super::*;

#[test]
fn each_approval_outcome_maps_to_the_answer_the_dialog_acts_on() {
    let cases = [
        (
            ServerApprovalOutcome::Saved {
                volume_id: Some("sftp-1".into()),
            },
            ServerApprovalResultView::Saved {
                volume_id: Some("sftp-1".into()),
            },
        ),
        (
            ServerApprovalOutcome::ListChanged,
            ServerApprovalResultView::ListChanged,
        ),
        (
            ServerApprovalOutcome::AlreadyAnswered,
            ServerApprovalResultView::AlreadyAnswered,
        ),
        (ServerApprovalOutcome::Unknown, ServerApprovalResultView::Unknown),
        (
            ServerApprovalOutcome::NotAServerGroup,
            ServerApprovalResultView::Unknown,
        ),
        (
            ServerApprovalOutcome::Refused(SaveRefusal::Server(SavedServerOutcome::RootNotFound)),
            ServerApprovalResultView::Refused {
                outcome: SavedServerOutcome::RootNotFound,
            },
        ),
        (
            ServerApprovalOutcome::Refused(SaveRefusal::AddServer(AddServerError::InvalidAddress {
                message: "x".into(),
            })),
            ServerApprovalResultView::AddServer {
                error: AddServerError::InvalidAddress { message: "x".into() },
            },
        ),
        (
            ServerApprovalOutcome::ServerChanged,
            ServerApprovalResultView::ServerChanged,
        ),
        (
            ServerApprovalOutcome::AlreadySaved {
                server_id: "sftp-1".into(),
            },
            ServerApprovalResultView::AlreadySaved {
                server_id: "sftp-1".into(),
            },
        ),
        (
            ServerApprovalOutcome::NameTaken { name: "NAS".into() },
            ServerApprovalResultView::NameTaken { name: "NAS".into() },
        ),
    ];
    for (outcome, view) in cases {
        assert_eq!(to_server_approval_view(outcome).expect("an answer"), view);
    }
}

/// A save that never answered may have written: the dialog re-reads, ❌ never offers the click
/// again.
#[test]
fn a_save_with_no_answer_is_the_approval_that_didnt_finish() {
    assert!(matches!(
        to_server_approval_view(ServerApprovalOutcome::NoAnswer),
        Err(SuggestedOpsError::ApprovalDidntFinish)
    ));
}

#[test]
fn the_saved_answer_serializes_as_the_dialog_reads_it() {
    let view = ServerApprovalResultView::Saved { volume_id: None };
    assert_eq!(
        serde_json::to_value(view).expect("serializes"),
        serde_json::json!({ "kind": "saved", "volumeId": null })
    );
    let view = ServerApprovalResultView::AlreadySaved {
        server_id: "sftp-1".into(),
    };
    assert_eq!(
        serde_json::to_value(view).expect("serializes"),
        serde_json::json!({ "kind": "alreadySaved", "serverId": "sftp-1" })
    );
}
