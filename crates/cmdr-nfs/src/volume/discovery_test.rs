use super::*;

const ROOT: NfsIdentity = NfsIdentity::new(0, 0);

fn id(uid: u32, gid: u32) -> NfsIdentity {
    NfsIdentity::new(uid, gid)
}

fn mac() -> NfsIdentity {
    id(501, 20)
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
    let guess = pick_identity(&owners, ROOT, mac());
    assert_eq!(guess.source, NfsIdentitySource::MostCommonOwner);
    assert_eq!((guess.identity.uid, guess.identity.gid), (1000, 100));
}

#[test]
fn a_tie_goes_to_the_lower_uid_so_the_answer_is_stable() {
    let guess = pick_identity(&[id(1001, 1), id(1000, 1)], ROOT, mac());
    assert_eq!(guess.identity.uid, 1000);
}

#[test]
fn with_only_root_owned_entries_the_roots_own_owner_and_then_this_mac() {
    let guess = pick_identity(&[id(0, 0)], id(1026, 100), mac());
    assert_eq!(guess.source, NfsIdentitySource::RootOwner);
    assert_eq!(guess.identity, id(1026, 100));
    let guess = pick_identity(&[], ROOT, mac());
    assert_eq!(guess.source, NfsIdentitySource::ThisMac);
    assert_eq!(guess.identity, mac());
}

#[test]
fn the_guessed_other_groups_are_the_ones_the_export_uses_most_common_first() {
    let owners = [
        id(1000, 100),
        id(1000, 100),
        id(1000, 100),
        // Someone else's file counts: a group on it is exactly how a person reads it.
        id(1001, 3000),
        id(1000, 2000),
        id(1001, 2000),
        id(1000, 4000),
        // Group 0 is never guessed, the same as uid 0.
        id(0, 0),
    ];
    let guess = pick_identity(&owners, id(0, 5000), mac());
    assert_eq!(guess.identity.gid, 100);
    assert_eq!(
        guess.identity.other_gids,
        vec![2000, 3000, 4000, 5000],
        "by count, ties to the lower gid, the primary out"
    );
}

#[test]
fn a_root_owned_folder_shared_with_a_group_still_guesses_that_group() {
    // `root:media 0750`, the shape of a NAS share: nothing has a non-zero owner.
    let guess = pick_identity(&[id(0, 2001), id(0, 2001)], ROOT, mac());
    assert_eq!(guess.source, NfsIdentitySource::ThisMac);
    assert_eq!(guess.identity.gid, 20);
    assert_eq!(guess.identity.other_gids, vec![2001]);
}

#[test]
fn the_root_owners_guess_takes_the_groups_on_the_entries() {
    let guess = pick_identity(&[id(0, 2001)], id(1026, 100), mac());
    assert_eq!(guess.source, NfsIdentitySource::RootOwner);
    assert_eq!(guess.identity.other_gids, vec![2001]);
}

#[test]
fn this_macs_guess_keeps_this_macs_own_groups_after_the_seen_ones() {
    let mut this_mac = mac();
    this_mac.other_gids = vec![12, 2001, 20, 80];
    let guess = pick_identity(&[id(0, 2001), id(0, 3000), id(0, 3000)], ROOT, this_mac);
    assert_eq!(guess.identity.other_gids, vec![3000, 2001, 12, 80]);
}

#[test]
fn another_persons_guess_never_borrows_this_macs_groups() {
    let mut this_mac = mac();
    this_mac.other_gids = vec![12, 80];
    let guess = pick_identity(&[id(1000, 100)], ROOT, this_mac);
    assert_eq!(guess.identity.other_gids, Vec::<u32>::new());
}

#[test]
fn the_guess_stops_at_the_rfc_cap_keeping_the_most_common() {
    let mut owners: Vec<NfsIdentity> = (2000..2030).map(|gid| id(1000, gid)).collect();
    owners.extend([id(1000, 2029), id(1000, 2029), id(1000, 2028)]);
    let guess = pick_identity(&owners, ROOT, mac());
    assert_eq!(guess.identity.gid, 2029);
    assert_eq!(guess.identity.other_gids.len(), 16);
    assert_eq!(guess.identity.other_gids[0], 2028);
}
