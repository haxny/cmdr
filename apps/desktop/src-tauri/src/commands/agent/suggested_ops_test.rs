//! Tests for `suggested_ops.rs`: the view mappings, the destination check, and the wire shapes.

use super::*;
use crate::agent::store::proposals::ProposalGroup;

/// Every refusal sends the user somewhere different, so the mapping is the thing worth
/// pinning: "somebody already answered" closes the group, "the list changed" sends them back
/// to re-read it, and an unmounted drive is neither. Collapsing any two would make the
/// dialog say the wrong thing in one of the cases.
#[test]
fn each_refusal_keeps_the_recovery_it_implies() {
    assert!(matches!(
        refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::NotPending {
            found: ProposalStatus::Approved
        })),
        ApprovalResultView::AlreadyAnswered
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Claim(ClaimRefusal::StaleStatus {
            found: ProposalStatus::Rejected
        })),
        ApprovalResultView::AlreadyAnswered
    ));
    assert!(
        matches!(
            refusal_view(ApprovalRefusal::Claim(ClaimRefusal::BindingMismatch {
                accepted: None,
                live: crate::agent::store::proposals::OpBinding {
                    op_count: 3,
                    digest: "abc".into()
                }
            })),
            ApprovalResultView::ListChanged
        ),
        "a changed op set is re-readable, never an already-answered group"
    );
    assert!(matches!(
        refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::Unknown)),
        ApprovalResultView::Unknown
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Claim(ClaimRefusal::Unknown)),
        ApprovalResultView::Unknown
    ));
}

/// Every refusal to START reaches the dialog as a reason it can word, ❌ never a sentence
/// it can only log (#184). An unconnected phone keeps the transfer dialogs' own variant, so
/// the suggestion reads exactly as a clicked copy off that phone would.
#[test]
fn a_refusal_to_start_carries_a_reason_the_dialog_can_word() {
    assert!(matches!(
        refusal_view(ApprovalRefusal::SourceUnavailable(
            WriteOperationError::SourceNotConnected {
                path: "/DCIM/one.jpg".into()
            }
        )),
        ApprovalResultView::Refused {
            error: WriteOperationError::SourceNotConnected { .. }
        }
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Rename(RenameStartError::Engine {
            error: WriteOperationError::SourceNoLongerConnected { path: "/a.png".into() }
        })),
        ApprovalResultView::Refused {
            error: WriteOperationError::SourceNoLongerConnected { .. }
        }
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Rename(RenameStartError::NothingToRename)),
        ApprovalResultView::NothingToRun
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::TargetMissing {
            verb: ProposalVerb::Move
        }),
        ApprovalResultView::CouldNotStart
    ));
}

fn group(id: i64, set_id: i64, verb: ProposalVerb) -> GroupSummary {
    GroupSummary {
        group: ProposalGroup {
            id,
            set_id,
            seq: 0,
            verb,
            status: ProposalStatus::Pending,
            source_volume_id: "root".into(),
            destination: Some("/Users/someone/Documents/Invoices".into()),
            destination_volume_id: None,
            reversible: Reversibility::RestoreMove,
            display_name: "five invoices".into(),
            rationale: Some("They all look like invoices.".into()),
            selector: None,
            created_at: 100,
            decided_at: None,
        },
        live_op_count: 5,
        total_op_count: 7,
    }
}

/// Read-time facts that say nothing beyond the destination state.
fn facts(destination_state: DestinationState) -> GroupReadFacts {
    GroupReadFacts {
        destination_state,
        destination_volume_name: None,
        source_folder: None,
        reversible: Reversibility::RestoreMove,
    }
}

fn listed(id: &str, name: &str, supports_trash: bool) -> crate::volume_listing::LocationInfo {
    crate::volume_listing::LocationInfo {
        id: id.to_string(),
        name: name.to_string(),
        path: format!("/Volumes/{id}"),
        category: crate::volume_listing::LocationCategory::AttachedVolume,
        icon: None,
        is_ejectable: false,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        fs_type: None,
        supports_trash,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    }
}

/// A trash group says "you can undo this" only when its source volume has a trash: on a NAS
/// share without one, the files are gone for good, and the dialog must say so (#747).
#[test]
fn a_trash_group_promises_undo_only_on_a_volume_with_a_trash() {
    let mut trash = group(1, 1, ProposalVerb::Trash).group;
    trash.destination = None;
    let volumes = [
        listed("root", "Macintosh HD", true),
        listed("smb-naspi", "naspi on 192.168.1.5", false),
    ];

    let on_root = read_facts(&trash, &volumes, DestinationState::NotApplicable, None);
    trash.source_volume_id = "smb-naspi".into();
    let on_share = read_facts(&trash, &volumes, DestinationState::NotApplicable, None);
    trash.source_volume_id = "nothing-lists-this".into();
    let on_unlisted = read_facts(&trash, &volumes, DestinationState::NotApplicable, None);

    assert_eq!(on_root.reversible, Reversibility::RestoreMove);
    assert_eq!(on_share.reversible, Reversibility::Irreversible);
    assert_eq!(
        on_unlisted.reversible,
        Reversibility::Irreversible,
        "unknown earns no promise"
    );
}

/// The header names the destination's volume as the switcher does, falling back to the source
/// volume for a rename (whose parent lives there), and names nothing for trash.
#[test]
fn the_destination_volume_is_named_as_the_switcher_names_it() {
    let volumes = [
        listed("root", "Macintosh HD", true),
        listed("smb-naspi", "naspi on 192.168.1.5", false),
    ];
    let mut moving = group(1, 1, ProposalVerb::Move).group;
    moving.destination_volume_id = Some("smb-naspi".into());
    let mut renaming = group(2, 1, ProposalVerb::Rename).group;
    renaming.destination_volume_id = None;
    let mut trashing = group(3, 1, ProposalVerb::Trash).group;
    trashing.destination = None;

    let name = |group: &ProposalGroup| {
        read_facts(group, &volumes, DestinationState::NotApplicable, None).destination_volume_name
    };
    assert_eq!(name(&moving).as_deref(), Some("naspi on 192.168.1.5"));
    assert_eq!(name(&renaming).as_deref(), Some("Macintosh HD"));
    assert_eq!(name(&trashing), None);
}

fn sweep(id: i64) -> ProposalSweep {
    ProposalSweep {
        id,
        conversation_id: Some(3),
        created_at: 100,
        created_by_model: Some("some-model".into()),
        rationale: Some("Ten new files in Downloads.".into()),
    }
}

/// Every field here is something the user weighs before approving. A mapping that dropped
/// one would take a disclosure off the screen while the dialog still looked complete,
/// which is the failure this whole surface exists to prevent.
#[test]
fn a_group_view_carries_every_fact_the_user_judges_it_by() {
    let view = to_group_view(
        group(7, 2, ProposalVerb::Move),
        GroupReadFacts {
            destination_state: DestinationState::WillBeCreated,
            destination_volume_name: Some("Macintosh HD".into()),
            source_folder: Some("/Users/someone/Downloads".into()),
            reversible: Reversibility::RestoreMove,
        },
    );

    assert_eq!(view.group_id, 7);
    assert_eq!(view.sweep_id, 2);
    assert_eq!(view.verb, ProposalVerb::Move);
    assert_eq!(view.status, ProposalStatus::Pending);
    assert_eq!(view.display_name, "five invoices");
    assert_eq!(view.rationale.as_deref(), Some("They all look like invoices."));
    assert_eq!(view.source_volume_id, "root");
    assert_eq!(view.destination.as_deref(), Some("/Users/someone/Documents/Invoices"));
    assert_eq!(view.destination_volume_name.as_deref(), Some("Macintosh HD"));
    assert_eq!(view.source_folder.as_deref(), Some("/Users/someone/Downloads"));
    assert_eq!(view.reversible, Reversibility::RestoreMove);
    assert_eq!(view.destination_state, DestinationState::WillBeCreated);
    assert_eq!(view.live_op_count, 5);
    assert_eq!(view.total_op_count, 7, "the deselected rows are part of the record");
    assert!(!view.from_selector);
}

/// A group a pattern produced says so, because "why is this file here?" has a different
/// answer for a list the user never watched being built.
#[test]
fn a_selector_built_group_reports_its_provenance() {
    let mut summary = group(1, 1, ProposalVerb::Trash);
    summary.group.selector = Some("{\"root\":\"~/Downloads\"}".into());

    assert!(to_group_view(summary, facts(DestinationState::NotApplicable)).from_selector);
}

/// An index that held nothing must stay absent all the way to the row. A zero would render
/// as an empty file and a 1970 date, both of which are claims nothing can back.
#[test]
fn an_op_with_no_snapshot_reports_absence_rather_than_zero() {
    let bare = to_op_view(ProposalOp {
        id: 4,
        group_id: 1,
        seq: 0,
        source_path: "/Users/someone/Downloads/one.dmg".into(),
        destination: None,
        status: OpStatus::Pending,
        snapshot_size: None,
        snapshot_mtime: None,
        snapshot_inode: None,
    });

    assert_eq!(bare.snapshot_size, None);
    assert_eq!(bare.snapshot_modified, None);
    let wire = serde_json::to_value(&bare).expect("serializes");
    assert!(wire["snapshotSize"].is_null());
    assert!(wire["snapshotModified"].is_null());
}

/// The marker answers for the verbs that create a folder, and stays quiet for the ones
/// where it would mean something else: a rename binds the parent its sources already sit
/// in, and a compress binds an archive path whose overwrite risk `reversible` already
/// carries.
#[test]
fn only_the_verbs_that_can_create_a_folder_answer_the_destination_question() {
    for verb in [ProposalVerb::Move, ProposalVerb::Copy, ProposalVerb::Extract] {
        assert!(destination_check_applies(verb), "{verb:?} binds a destination folder");
    }
    for verb in [
        ProposalVerb::Rename,
        ProposalVerb::Compress,
        ProposalVerb::Trash,
        ProposalVerb::Delete,
    ] {
        assert!(
            !destination_check_applies(verb),
            "{verb:?} must not claim to create one"
        );
    }
}

fn move_to(destination_volume_id: &str, destination: &str) -> ProposalGroup {
    let mut summary = group(1, 1, ProposalVerb::Move);
    summary.group.destination = Some(destination.into());
    summary.group.destination_volume_id = Some(destination_volume_id.into());
    summary.group
}

/// The QA repro (#747): four moves onto a NAS share the switcher showed as `saved` all read
/// "Cmdr couldn't check the target folder". A `saved` row is published only for a share whose
/// volume ISN'T registered, so the lookup found nothing and the check answered `Unknown`
/// without a word. Nothing connected is its own answer, and approving would refuse on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_on_a_saved_share_nobody_connected_says_so() {
    use crate::network::known_shares::{self, AuthOptions, ConnectionMode, KnownNetworkShare};
    let volume_id = "smb-198-51-100-62-445-naspi-suggested-ops-test";
    known_shares::remember_share(KnownNetworkShare {
        server_name: "198.51.100.62".to_string(),
        share_name: "naspi".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-26T00:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Guest,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: None,
        address: Some("198.51.100.62".to_string()),
        port: Some(445),
        volume_id: Some(volume_id.to_string()),
        mount_path: Some("/Volumes/naspi-suggested-ops-test".to_string()),
        pinned: true,
    });

    let state = destination_state(&move_to(volume_id, "/Volumes/naspi-suggested-ops-test/Installers")).await;

    assert_eq!(state, DestinationState::NotConnected);
}

/// A connected destination answers from its own volume: there, or created by approving.
#[tokio::test(flavor = "multi_thread")]
async fn a_connected_destination_answers_whether_the_folder_is_there() {
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_fs::volume::{InMemoryVolume, Volume};
    let volume_id = "suggested-ops-destination-test";
    let volume = InMemoryVolume::new("Share").with_root("/Volumes/share-test");
    volume
        .create_directory(Path::new("/Volumes/share-test/Invoices"))
        .await
        .expect("seed folder");
    let _registration = TestVolumeRegistration::install(volume_id, std::sync::Arc::new(volume));

    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/share-test/Invoices")).await,
        DestinationState::Exists
    );
    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/share-test/Receipts")).await,
        DestinationState::WillBeCreated
    );
}

/// A volume that refuses the check (a dropped session, a permission wall) doesn't know, and
/// must not read as "will be created".
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_whose_volume_refuses_the_check_is_unknown() {
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_fs::volume::{InMemoryVolume, Volume};
    let volume_id = "suggested-ops-destination-refusing-test";
    let volume = InMemoryVolume::new("Share").with_root("/Volumes/refusing-test");
    volume
        .create_directory(Path::new("/Volumes/refusing-test/Invoices"))
        .await
        .expect("seed folder");
    volume.set_stat_failing(Path::new("/Volumes/refusing-test/Invoices"));
    let _registration = TestVolumeRegistration::install(volume_id, std::sync::Arc::new(volume));

    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/refusing-test/Invoices")).await,
        DestinationState::Unknown
    );
}

/// A sweep is how the user sees one thing the agent noticed, so its groups arrive under one
/// header however many there are.
#[test]
fn groups_fold_under_the_sweep_that_produced_them() {
    let rows = vec![
        (
            to_group_view(group(1, 10, ProposalVerb::Move), facts(DestinationState::Exists)),
            Some(sweep(10)),
        ),
        (
            to_group_view(
                group(2, 10, ProposalVerb::Trash),
                facts(DestinationState::NotApplicable),
            ),
            Some(sweep(10)),
        ),
        (
            to_group_view(
                group(3, 11, ProposalVerb::Delete),
                facts(DestinationState::NotApplicable),
            ),
            Some(sweep(11)),
        ),
    ];

    let sweeps = group_into_sweeps(rows);

    assert_eq!(sweeps.len(), 2, "two wakes, not three groups: {sweeps:?}");
    assert_eq!(sweeps[0].sweep_id, 10);
    assert_eq!(sweeps[0].groups.len(), 2);
    assert_eq!(sweeps[0].rationale.as_deref(), Some("Ten new files in Downloads."));
    assert_eq!(sweeps[1].sweep_id, 11);
    assert_eq!(sweeps[1].groups.len(), 1);
}

/// A sweep row missing under its own groups can't happen through any path here, but
/// dropping the groups would hide work the user is being asked about. They get a
/// placeholder header instead.
#[test]
fn groups_whose_sweep_row_is_missing_are_still_shown() {
    let rows = vec![(
        to_group_view(group(1, 42, ProposalVerb::Move), facts(DestinationState::Exists)),
        None,
    )];

    let sweeps = group_into_sweeps(rows);

    assert_eq!(sweeps.len(), 1);
    assert_eq!(sweeps[0].sweep_id, 42);
    assert_eq!(sweeps[0].groups.len(), 1);
    assert_eq!(sweeps[0].rationale, None);
}

/// "The store isn't open" (agent features off) and "the store refused" (a
/// locked database) call for different answers from the dialog, so they stay
/// separate variants rather than two spellings of one sentence.
#[test]
fn a_store_that_isnt_open_is_not_a_store_that_refused() {
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::StoreNotOpen).unwrap(),
        serde_json::json!({ "type": "storeNotOpen" })
    );
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::Store {
            detail: "database is locked".to_string()
        })
        .unwrap(),
        serde_json::json!({ "type": "store", "detail": "database is locked" })
    );
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::ApprovalDidntFinish).unwrap(),
        serde_json::json!({ "type": "approvalDidntFinish" })
    );
}
