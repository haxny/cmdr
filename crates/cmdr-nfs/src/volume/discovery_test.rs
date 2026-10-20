use super::*;

const MAC: NfsIdentity = NfsIdentity { uid: 501, gid: 20 };
const ROOT: NfsIdentity = NfsIdentity { uid: 0, gid: 0 };

fn id(uid: u32, gid: u32) -> NfsIdentity {
    NfsIdentity { uid, gid }
}

#[test]
fn the_most_common_non_zero_owner_wins_over_a_root_owned_export_root() {
    let owners = [
        id(1000, 100),
        id(0, 0),
        id(1000, 100),
        id(1001, 100),
        id(1000, 1000),
        id(0, 0),
        id(0, 0),
    ];
    let guess = pick_identity(&owners, ROOT, MAC);
    assert_eq!(guess.source, NfsIdentitySource::MostCommonOwner);
    assert_eq!(guess.identity, id(1000, 100));
}

#[test]
fn a_tie_goes_to_the_lower_uid_so_the_answer_is_stable() {
    let guess = pick_identity(&[id(1001, 1), id(1000, 1)], ROOT, MAC);
    assert_eq!(guess.identity.uid, 1000);
}

#[test]
fn with_only_root_owned_entries_the_roots_own_owner_and_then_this_mac() {
    let guess = pick_identity(&[id(0, 0)], id(1026, 100), MAC);
    assert_eq!(guess.source, NfsIdentitySource::RootOwner);
    assert_eq!(guess.identity, id(1026, 100));
    let guess = pick_identity(&[], ROOT, MAC);
    assert_eq!(guess.source, NfsIdentitySource::ThisMac);
    assert_eq!(guess.identity, MAC);
}
