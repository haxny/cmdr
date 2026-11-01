use crate::test_tree::Tree;
use crate::*;

const T: u64 = 1_700_000_000;

fn settings() -> GuardSettings {
    GuardSettings::default()
}

struct Case {
    plan: Plan,
    left: Snapshot,
    right: Snapshot,
    settings: GuardSettings,
    report: GuardReport,
}

impl Case {
    /// Gates `plan` (this case's or one derived from it) against this case's
    /// snapshots and settings.
    fn gate(&self, plan: &Plan, acknowledged: &[Acknowledgement]) -> Gated {
        gate(plan, &self.left, &self.right, &self.settings, acknowledged)
    }
}

fn run(mode: Mode, left: Tree, right: Tree, settings: &GuardSettings) -> Case {
    let (left, right) = (left.build(), right.build());
    let options = PlanOptions {
        mode,
        rules: CompareRules::QUICK,
    };
    let plan = plan(&left, &right, None, &options);
    let report = check(&plan, &left, &right, settings);
    Case {
        plan,
        left,
        right,
        settings: settings.clone(),
        report,
    }
}

/// `total` files on the right, the first `kept` of them also on the left.
fn mirror_removing(kept: usize, total: usize) -> (Tree, Tree) {
    let mut left = Tree::new();
    let mut right = Tree::new();
    for n in 0..total {
        let name = format!("f{n:02}");
        right = right.file(&name, 1, T);
        if n < kept {
            left = left.file(&name, 1, T);
        }
    }
    (left, right)
}

fn removals(plan: &Plan) -> Vec<u32> {
    plan.items
        .iter()
        .filter(|item| item.action == Action::RemoveRight)
        .map(|item| item.index)
        .collect()
}

#[test]
fn guard_defaults_are_half_and_off() {
    let defaults = settings();
    assert_eq!(defaults.max_remove_percent, 50);
    assert_eq!(defaults.max_remove_count, 0);
    assert_eq!(defaults.sentinel, None);
}

#[test]
fn max_remove_fires_only_above_the_threshold() {
    // 50% of 10 files: 5 removals pass, 6 block.
    let (left, right) = mirror_removing(5, 10);
    let report = run(Mode::Mirror, left, right, &settings()).report;
    assert!(report.blocks.is_empty(), "{:?}", report.blocks);

    let (left, right) = mirror_removing(4, 10);
    let report = run(Mode::Mirror, left, right, &settings()).report;
    assert_eq!(
        report.blocks,
        vec![GuardBlock::MaxRemove {
            side: Side::Right,
            removals: 6,
            target_files: 10
        }]
    );
}

#[test]
fn max_remove_count_fires_above_its_count_and_zero_is_off() {
    let (left, right) = mirror_removing(16, 20);
    let by_count = GuardSettings {
        max_remove_count: 3,
        ..settings()
    };
    let report = run(Mode::Mirror, left.clone(), right.clone(), &by_count).report;
    assert!(matches!(report.blocks[..], [GuardBlock::MaxRemove { removals: 4, .. }]));
    let at_count = GuardSettings {
        max_remove_count: 4,
        ..settings()
    };
    assert!(run(Mode::Mirror, left, right, &at_count).report.blocks.is_empty());
}

#[test]
fn removing_a_folder_counts_its_files() {
    let left = Tree::new().file("keep", 1, T);
    let right = Tree::new().file("keep", 1, T).file("old/a", 1, T).file("old/b", 1, T);
    let case = run(Mode::Mirror, left, right, &settings());
    let (plan, report) = (&case.plan, &case.report);
    assert_eq!(removals(plan).len(), 1, "one folder item");
    assert!(matches!(
        report.blocks[..],
        [GuardBlock::MaxRemove {
            removals: 2,
            target_files: 3,
            ..
        }]
    ));
}

#[test]
fn remove_anyway_releases_max_remove() {
    let (left, right) = mirror_removing(4, 10);
    let left = left.file("new", 1, T);
    let case = run(Mode::Mirror, left, right, &settings());
    let plan = &case.plan;
    let removing = removals(plan);
    let gated = case.gate(plan, &[]);
    assert_eq!(gated.held.len(), 6);
    assert!(
        gated
            .held
            .iter()
            .all(|(_, block)| matches!(block, GuardBlock::MaxRemove { .. }))
    );
    assert_eq!(gated.run.len(), 1, "the copy still runs");

    let gated = case.gate(plan, &[Acknowledgement::RemoveAnyway]);
    assert!(gated.held.is_empty());
    assert!(removing.iter().all(|index| gated.run.contains(index)));
    assert_eq!(gated.run.len(), 7);
}

#[test]
fn empty_source_root_holds_every_removal() {
    // `/Volumes/Backup` unmounted: the left lists empty, the right is full.
    let right = Tree::new().file("a", 1, T).file("b", 1, T).file("c/d", 1, T);
    let lax = GuardSettings {
        max_remove_percent: 100,
        ..settings()
    };
    let case = run(Mode::Mirror, Tree::new(), right, &lax);
    let (plan, report) = (&case.plan, &case.report);
    assert_eq!(removals(plan).len(), 3);
    assert!(report.blocks.contains(&GuardBlock::EmptyRoot { side: Side::Left }));
    let acks = [Acknowledgement::RemoveAnyway, Acknowledgement::CopyIntoEmpty];
    let gated = case.gate(plan, &acks);
    assert!(gated.run.is_empty(), "{:?}", gated.run);
    assert_eq!(gated.held.len(), 3);
    assert!(
        gated
            .held
            .iter()
            .all(|(_, block)| *block == GuardBlock::EmptyRoot { side: Side::Left })
    );
}

#[test]
fn empty_target_root_holds_removals_too() {
    let case = run(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new(), &settings());
    let plan = apply_overrides(
        &case.plan,
        &[ActionChange {
            item: 0,
            action: Action::RemoveLeft,
        }],
    )
    .unwrap();
    let gated = case.gate(&plan, &[Acknowledgement::RemoveAnyway, Acknowledgement::CopyIntoEmpty]);
    assert!(gated.run.is_empty());
    assert_eq!(gated.held, vec![(0, GuardBlock::EmptyRoot { side: Side::Right })]);
}

#[test]
fn copies_into_an_empty_root_need_acknowledgement() {
    for mode in [Mode::BothWays, Mode::Update, Mode::Mirror] {
        let left = Tree::new().file("a", 1, T).file("b", 1, T).file("c/d", 1, T);
        let case = run(mode, left, Tree::new(), &settings());
        let (plan, report) = (&case.plan, &case.report);
        assert_eq!(
            report.blocks,
            vec![GuardBlock::EmptyRoot { side: Side::Right }],
            "{mode:?}"
        );
        let gated = case.gate(plan, &[]);
        assert!(gated.run.is_empty());
        assert_eq!(gated.held.len(), 3);
        let gated = case.gate(plan, &[Acknowledgement::CopyIntoEmpty]);
        assert_eq!(gated.run, vec![0, 1, 2]);
        assert!(gated.held.is_empty());
    }
}

#[test]
fn missing_sentinel_holds_everything() {
    let with_sentinel = GuardSettings {
        sentinel: Some(".sync-here".to_owned()),
        ..settings()
    };
    let left = Tree::new().file(".sync-here", 0, T).file("a", 1, T);
    let right = Tree::new().file("b", 1, T).file("c", 1, T).file("d", 1, T);
    let case = run(Mode::BothWays, left.clone(), right, &with_sentinel);
    let (plan, report) = (&case.plan, &case.report);
    assert_eq!(report.blocks, vec![GuardBlock::SentinelMissing { side: Side::Right }]);
    let acks = [Acknowledgement::RemoveAnyway, Acknowledgement::CopyIntoEmpty];
    let gated = case.gate(plan, &acks);
    assert!(gated.run.is_empty());
    assert_eq!(gated.held.len(), 5, "every copy, the sentinel's own included");

    let right = Tree::new().file(".sync-here", 0, T).file("b", 1, T);
    let report = run(Mode::BothWays, left, right, &with_sentinel).report;
    assert!(report.blocks.is_empty());
}

#[test]
fn uniform_offset_reports_clock_skew() {
    // 95% of pairs off by +3600 s (the right clock runs an hour ahead).
    let mut left = Tree::new();
    let mut right = Tree::new();
    for n in 0..20u64 {
        let name = format!("f{n:02}");
        let offset = if n == 0 { 7 } else { 3600 };
        left = left.file(&name, 5, T + n * 10);
        right = right.file(&name, 5, T + n * 10 + offset);
    }
    let report = run(Mode::BothWays, left.clone(), right, &settings()).report;
    assert_eq!(
        report.skew,
        Some(ClockSkew {
            offset_secs: 3600,
            share_percent: 95
        })
    );

    // 1234 s is not a clock step (not a multiple of 15 min).
    let mut odd = Tree::new();
    for n in 0..20u64 {
        odd = odd.file(&format!("f{n:02}"), 5, T + n * 10 + 1234);
    }
    assert_eq!(run(Mode::BothWays, left.clone(), odd, &settings()).report.skew, None);

    // Below 90%: 17 of 20 shifted.
    let mut mixed = Tree::new();
    for n in 0..20u64 {
        let offset = if n < 3 { 0 } else { -1800i64 };
        let mtime = (T + n * 10).checked_add_signed(offset).unwrap();
        mixed = mixed.file(&format!("f{n:02}"), 5, mtime);
    }
    assert_eq!(run(Mode::BothWays, left, mixed, &settings()).report.skew, None);
}

#[test]
fn negative_offsets_and_the_26_hour_cap() {
    let mut left = Tree::new();
    let mut behind = Tree::new();
    let mut too_far = Tree::new();
    for n in 0..10u64 {
        let name = format!("f{n}");
        left = left.file(&name, 5, T + n);
        behind = behind.file(&name, 5, T + n - 1800);
        too_far = too_far.file(&name, 5, T + n + 27 * 3600);
    }
    let report = run(Mode::Mirror, left.clone(), behind, &settings()).report;
    assert_eq!(report.skew.map(|skew| skew.offset_secs), Some(-1800));
    assert_eq!(run(Mode::Mirror, left, too_far, &settings()).report.skew, None);
}

#[test]
fn older_overwrites_newer_is_counted() {
    let left = Tree::new().file("a", 1, T).file("b", 1, T);
    let right = Tree::new().file("a", 2, T + 100).file("b", 2, T + 100);
    let report = run(Mode::Mirror, left, right, &settings()).report;
    assert_eq!(report.older_overwrites_newer, 2);
    assert!(report.blocks.is_empty(), "a warning, never a block");
}

#[test]
fn gate_rechecks_a_plan_changed_after_its_report() {
    // Compare only plans nothing; the report says all clear. Then six removals are set by hand.
    let (_, right) = mirror_removing(0, 10);
    let left = Tree::new().file("x", 1, T);
    let case = run(Mode::CompareOnly, left, right, &settings());
    assert!(case.report.blocks.is_empty());
    let changes: Vec<ActionChange> = case
        .plan
        .items
        .iter()
        .filter(|item| item.right.is_some())
        .take(6)
        .map(|item| ActionChange {
            item: item.index,
            action: Action::RemoveRight,
        })
        .collect();
    let changed = apply_overrides(&case.plan, &changes).unwrap();
    let gated = case.gate(&changed, &[]);
    assert!(gated.run.is_empty(), "the stale all-clear must not let them through");
    assert_eq!(gated.held.len(), 6);
    assert!(gated.held.iter().all(|(_, block)| matches!(
        block,
        GuardBlock::MaxRemove {
            side: Side::Right,
            removals: 6,
            ..
        }
    )));
}

fn shifted_pairs(pairs: u64, shift: i64) -> (Tree, Tree) {
    (0..pairs).fold((Tree::new(), Tree::new()), |(left, right), n| {
        let name = format!("f{n:02}");
        let mtime = (T + n * 10).checked_add_signed(shift).unwrap();
        (left.file(&name, 5, T + n * 10), right.file(&name, 5, mtime))
    })
}

#[test]
fn skew_needs_ten_pairs() {
    let (left, right) = shifted_pairs(9, 1800);
    assert_eq!(run(Mode::BothWays, left, right, &settings()).report.skew, None);
    let (left, right) = shifted_pairs(10, 1800);
    let skew = run(Mode::BothWays, left, right, &settings()).report.skew;
    assert_eq!(skew.map(|skew| skew.offset_secs), Some(1800));
}

#[test]
fn hour_shifts_are_not_skew_when_the_hour_shift_option_is_on() {
    let (left, right) = shifted_pairs(20, 3600);
    let (left, right) = (left.build(), right.build());
    let options = PlanOptions {
        mode: Mode::BothWays,
        rules: CompareRules {
            ignore_hour_shift: true,
            ..CompareRules::QUICK
        },
    };
    let plan = plan(&left, &right, None, &options);
    assert_eq!(check(&plan, &left, &right, &settings()).skew, None);
}
