//! The bounds a 1M-file job holds the tree to: bytes per node, and the time a
//! window over a million rows takes.

use std::time::{Duration, Instant};

use super::build::{BuilderId, TreeBuilder};
use super::model::{NodeKind, TreeModel};
use super::order::TreeOrder;
use super::view::{TransferTreeViewRequest, TreeWindow, flatten};

const FOLDERS: usize = 1_000;
const FILES_PER_FOLDER: usize = 999;

/// A camera-roll-shaped job: a thousand dated folders of a thousand-odd photos.
fn camera_roll() -> TreeModel {
    let mut builder = TreeBuilder::new();
    let top = builder.child(BuilderId::ROOT, "Photos", NodeKind::Folder, 0);
    for folder in 0..FOLDERS {
        let day = builder.child(
            top,
            &format!("2024-{:02}-{:02} trip", folder % 12 + 1, folder % 28 + 1),
            NodeKind::Folder,
            0,
        );
        for file in 0..FILES_PER_FOLDER {
            builder.child(
                day,
                &format!("IMG_2024{folder:04}_{file:06}.jpg"),
                NodeKind::File,
                2_000_000 + file as u64,
            );
        }
    }
    builder.finish(&TreeOrder::bytewise()).0
}

#[test]
fn a_million_node_tree_stays_within_64_bytes_a_node_names_included() {
    let model = camera_roll();
    let nodes = model.len();
    assert!(nodes > 1_000_000, "node count: {nodes}");

    // 47.0 B/node measured (2026-10-11, these 23-byte names, debug build).
    let per_node = model.heap_bytes() as f64 / nodes as f64;
    assert!(per_node <= 64.0, "{per_node:.1} bytes per node");
}

#[test]
fn a_window_over_a_million_flat_rows_answers_without_walking_them() {
    let mut builder = TreeBuilder::new();
    for file in 0..1_000_000 {
        builder.child(BuilderId::ROOT, &format!("f{file:07}"), NodeKind::File, 1);
    }
    let model = builder.finish(&TreeOrder::bytewise()).0;
    let request = TransferTreeViewRequest {
        expanded: Vec::new(),
        collapsed: Vec::new(),
        window: TreeWindow::At { offset: 987_654 },
        limit: 200,
    };

    let started = Instant::now();
    let window = flatten(&model, &request);
    let took = started.elapsed();

    assert_eq!(window.total_rows, 1_000_000);
    assert_eq!(window.rows[0].label, "f0987654");
    assert_eq!(window.rows.len(), 200);
    // Microseconds in a release build; the bound is for an unoptimized test
    // build on a loaded CI runner. A walk over the million rows takes far longer.
    assert!(took < Duration::from_millis(50), "{took:?}");
}
