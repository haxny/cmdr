//! A compress's rows follow its producer's ticks, entry by entry.

use super::*;
use crate::file_system::write_operations::transfer_tree::test_support::{node_at, row_states};
use crate::file_system::write_operations::transfer_tree::{
    TransferTreeSlot, TreeLanding, TreeOrder, from_archive_plan,
};

/// The compress's rows follow the plan: the producer writes entries one at a
/// time in plan order, and its ticks alone say which one is in flight. A folder
/// entry is in place once written, an empty file lands without a chunk, and
/// the file being read shows the bytes read since the entry before it ended.
#[test]
fn compress_rows_follow_the_producers_ticks_entry_by_entry() {
    let plan = [
        ("top/", true, 0),
        ("top/a.txt", false, 10),
        ("top/empty.txt", false, 0),
        ("top/z.txt", false, 30),
    ];
    let (model, entry_rows) = from_archive_plan(
        plan.iter().copied(),
        &TreeOrder::active(crate::file_system::write_operations::installed().host()),
    )
    .expect("the tree builds");
    let slot = TransferTreeSlot::default();
    slot.install(Some(model), TreeLanding::nowhere());
    let rows = CompressRows::new(
        slot.reporter(),
        entry_rows
            .into_iter()
            .zip(plan.iter().map(|(_, is_dir, _)| *is_dir))
            .collect(),
    );
    let tree = slot.get().expect("installed");
    let tick = |entries_done, source_bytes_done| {
        rows.on_tick(FreshZipProgress {
            entries_done,
            source_bytes_done,
        });
    };
    let a = tree.read(|model| node_at(model, &["top", "a.txt"])).expect("on");

    tick(1, 0); // the folder entry
    tick(1, 4); // 4 bytes into a.txt
    assert_eq!(tree.read(|model| model.in_flight_bytes(a)).flatten(), Some(4));
    tick(2, 10); // a.txt done
    tick(3, 10); // empty.txt done without a chunk
    tick(3, 25); // 15 bytes into z.txt

    assert_eq!(
        tree.read(row_states).expect("on"),
        vec!["top/ active", "  a.txt done", "  empty.txt done", "  z.txt active"]
    );
    let z = tree.read(|model| node_at(model, &["top", "z.txt"])).expect("on");
    assert_eq!(tree.read(|model| model.in_flight_bytes(z)).flatten(), Some(15));

    tick(4, 40);
    assert_eq!(
        tree.read(row_states).expect("on"),
        vec!["top/ done", "  a.txt done", "  empty.txt done", "  z.txt done"]
    );
}
