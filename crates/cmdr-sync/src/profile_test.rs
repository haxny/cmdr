use crate::*;

fn defaults() -> SyncDefaults {
    SyncDefaults {
        keep: KeepPolicy::default(),
        time_tolerance_secs: 2,
        ignore_hour_shift: false,
        guards: GuardSettings::default(),
    }
}

fn root(volume: &str, path: &str) -> RootRef {
    RootRef {
        volume_id: volume.to_owned(),
        path: path.to_owned(),
    }
}

fn profile() -> SyncProfile {
    SyncProfile {
        id: "p1".to_owned(),
        name: "Photos".to_owned(),
        left: root("root", "/a"),
        right: root("root", "/b"),
        options: SyncOptions::default(),
        show_filters: ShowFilters::default(),
        restore_show_filters: false,
        overrides: ProfileOverrides::default(),
    }
}

#[test]
fn inherit_follows_settings_and_override_wins() {
    let d = defaults();
    let o = SyncOptions::default();
    let inherited = effective(&d, &o, &ProfileOverrides::default());
    assert_eq!(inherited.keep, d.keep);
    assert_eq!(inherited.rules.tolerance_secs, 2);
    assert!(!inherited.rules.ignore_hour_shift);
    assert_eq!(inherited.guards, d.guards);

    // A change in Settings reaches a profile that didn't override it.
    let changed = SyncDefaults {
        time_tolerance_secs: 5,
        ignore_hour_shift: true,
        ..defaults()
    };
    let follows = effective(&changed, &o, &ProfileOverrides::default());
    assert_eq!(follows.rules.tolerance_secs, 5);
    assert!(follows.rules.ignore_hour_shift);

    let own_keep = KeepPolicy {
        place: KeepPlace::TrashOnly,
        ..KeepPolicy::default()
    };
    let own_guards = GuardSettings {
        max_remove_percent: 10,
        ..GuardSettings::default()
    };
    let overrides = ProfileOverrides {
        keep: Some(own_keep.clone()),
        time_tolerance_secs: Some(0),
        ignore_hour_shift: Some(true),
        time_offset_secs: 0,
        guards: Some(own_guards.clone()),
    };
    let own = effective(&changed, &o, &overrides);
    assert_eq!(own.keep, own_keep);
    assert_eq!(own.rules.tolerance_secs, 0);
    assert!(own.rules.ignore_hour_shift);
    assert_eq!(own.guards, own_guards);
}

#[test]
fn options_reach_the_rules() {
    let o = SyncOptions {
        compare_by: CompareBy::SizeOnly,
        ignore_date: true,
        ..SyncOptions::default()
    };
    let rules = effective(&defaults(), &o, &ProfileOverrides::default()).rules;
    assert_eq!(rules.by, CompareBy::SizeOnly);
    assert!(rules.ignore_date);
}

#[test]
fn time_offset_reaches_the_rules() {
    let o = SyncOptions::default();
    let overrides = ProfileOverrides {
        time_offset_secs: 3600,
        ..Default::default()
    };
    assert_eq!(effective(&defaults(), &o, &overrides).rules.right_offset_secs, 3600);
}

#[test]
fn defaults_match_the_decisions() {
    let o = SyncOptions::default();
    assert_eq!(o.mode, Mode::BothWays);
    assert_eq!(o.compare_by, CompareBy::DateAndSize);
    assert!(o.subfolders);
    assert!(!o.only_selected && !o.ignore_date && !o.empty_folders);
    assert_eq!(o.masks, "");
    assert_eq!(
        o.excludes,
        DEFAULT_EXCLUDES.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
    );
    assert_eq!(ProfileOverrides::default().time_offset_secs, 0);
}

#[test]
fn a_good_profile_validates() {
    assert_eq!(validate(&profile()), Ok(()));
}

#[test]
fn empty_name_is_a_typed_problem() {
    let p = SyncProfile {
        name: "   ".to_owned(),
        ..profile()
    };
    assert_eq!(validate(&p), Err(ProfileProblem::EmptyName));
}

#[test]
fn bad_mask_is_a_typed_problem() {
    let mut p = profile();
    p.options.masks = "*.txt | *.tmp | *.bak".to_owned();
    assert!(matches!(
        validate(&p),
        Err(ProfileProblem::BadMask {
            error: SyncMaskError::SecondBar { .. }
        })
    ));
}

#[test]
fn tolerance_and_percent_are_range_checked() {
    let mut p = profile();
    p.overrides.time_tolerance_secs = Some(61);
    assert_eq!(validate(&p), Err(ProfileProblem::ToleranceOutOfRange { secs: 61 }));
    p.overrides.time_tolerance_secs = Some(60);
    assert_eq!(validate(&p), Ok(()));
    p.overrides.guards = Some(GuardSettings {
        max_remove_percent: 0,
        ..GuardSettings::default()
    });
    assert_eq!(validate(&p), Err(ProfileProblem::PercentOutOfRange { percent: 0 }));
    p.overrides.guards = Some(GuardSettings {
        max_remove_percent: 100,
        ..GuardSettings::default()
    });
    assert_eq!(
        validate(&p),
        Err(ProfileProblem::PercentOutOfRange { percent: 100 }),
        "100 would switch the guard off"
    );
    p.overrides.guards = Some(GuardSettings {
        max_remove_count: 1_000_001,
        ..GuardSettings::default()
    });
    assert_eq!(validate(&p), Err(ProfileProblem::CountOutOfRange { count: 1_000_001 }));
}

#[test]
fn nested_roots_on_one_volume_conflict() {
    let c = |l: RootRef, r: RootRef| roots_conflict(&l, &r);
    assert_eq!(
        c(root("v", "/a"), root("v", "/a/b")),
        Some(RootConflict::Nested { outer: Side::Left })
    );
    assert_eq!(
        c(root("v", "/a/b/"), root("v", "/a")),
        Some(RootConflict::Nested { outer: Side::Right })
    );
    assert_eq!(c(root("v", "/a"), root("v", "/a/")), Some(RootConflict::SameFolder));
    assert_eq!(c(root("v", "/a"), root("v", "/ab")), None);
    assert_eq!(c(root("v", "/a"), root("w", "/a/b")), None);
    assert_eq!(c(root("v", "/a"), root("w", "/a")), None);
}

#[test]
fn keep_policy_check_names_the_refusal() {
    assert_eq!(check_keep_policy(&KeepPolicy::default()), Ok(()));
    let outside = |folder_name: &str| KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::TargetRoot {
            folder_name: folder_name.to_owned(),
        },
    };
    assert_eq!(check_keep_policy(&outside("versions")), Err(KeepRefusal::BadTemplate));
    assert_eq!(
        check_keep_policy(&outside(".cmdr-sync/../x")),
        Err(KeepRefusal::EscapesFolder)
    );
    let custom = |template: &str| KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: template.to_owned(),
        },
    };
    assert_eq!(check_keep_policy(&custom("")), Err(KeepRefusal::BadTemplate));
    assert_eq!(
        check_keep_policy(&custom("/Backups/{nope}")),
        Err(KeepRefusal::BadTemplate)
    );
    assert_eq!(check_keep_policy(&custom("/Backups/{profile}/{root}")), Ok(()));
}

#[test]
fn a_bad_keep_override_is_a_typed_problem() {
    let mut p = profile();
    p.overrides.keep = Some(KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: String::new(),
        },
    });
    assert_eq!(
        validate(&p),
        Err(ProfileProblem::BadKeep {
            refusal: KeepRefusal::BadTemplate
        })
    );
}

#[test]
fn out_of_range_defaults_are_clamped_not_trusted() {
    let wild = SyncDefaults {
        time_tolerance_secs: 86_400,
        guards: GuardSettings {
            max_remove_percent: 200,
            max_remove_count: u32::MAX,
            ..GuardSettings::default()
        },
        ..defaults()
    };
    assert_eq!(
        validate_defaults(&wild),
        Err(ProfileProblem::ToleranceOutOfRange { secs: 86_400 })
    );
    let e = effective(&wild, &SyncOptions::default(), &ProfileOverrides::default());
    assert_eq!(e.rules.tolerance_secs, MAX_TOLERANCE_SECS);
    assert_eq!(e.guards.max_remove_percent, MAX_REMOVE_PERCENT);
    assert_eq!(e.guards.max_remove_count, MAX_REMOVE_COUNT);
    let zero = SyncDefaults {
        guards: GuardSettings {
            max_remove_percent: 0,
            ..GuardSettings::default()
        },
        ..defaults()
    };
    assert_eq!(
        effective(&zero, &SyncOptions::default(), &ProfileOverrides::default())
            .guards
            .max_remove_percent,
        1
    );
    assert_eq!(validate_defaults(&defaults()), Ok(()));
}
