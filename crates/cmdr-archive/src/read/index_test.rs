use super::*;

fn file_seed(path: &str, size: u64) -> NodeSeed {
    NodeSeed {
        path: path.to_string(),
        is_dir: false,
        is_symlink: false,
        size: Some(size),
        compressed_size: Some(size),
        modified: Some(1_700_000_000),
        encrypted: false,
        mode: None,
    }
}

fn dir_seed(path: &str) -> NodeSeed {
    NodeSeed {
        path: path.to_string(),
        is_dir: true,
        is_symlink: false,
        size: None,
        compressed_size: None,
        modified: Some(1_700_000_500),
        encrypted: false,
        mode: None,
    }
}

fn child_names(children: &HashMap<String, Vec<String>>, dir: &str) -> Vec<String> {
    children
        .get(dir)
        .map(|v| v.iter().map(|p| leaf_name(p).to_string()).collect())
        .unwrap_or_default()
}

/// Builds the tree with an effectively-unlimited node cap (the node cap is
/// tested separately by passing a small one).
fn build(seeds: Vec<NodeSeed>) -> BuiltTree {
    build_tree(seeds, usize::MAX).expect("uncapped build should not exceed the node cap")
}

#[test]
fn synthesizes_implied_directories_from_file_prefixes() {
    // No explicit dir entries: `a/b/c.txt` must imply `a/` and `a/b/`.
    let (nodes, children) = build(vec![file_seed("a/b/c.txt", 10)]);

    assert!(nodes.get("a").is_some_and(|n| n.is_dir), "a/ should be synthesized");
    assert!(nodes.get("a/b").is_some_and(|n| n.is_dir), "a/b/ should be synthesized");
    assert!(nodes.get("a/b/c.txt").is_some_and(|n| !n.is_dir));

    // Synthetic dirs carry no timestamp.
    assert_eq!(nodes["a"].modified, None);
    assert_eq!(nodes["a/b"].modified, None);

    assert_eq!(child_names(&children, ""), vec!["a"]);
    assert_eq!(child_names(&children, "a"), vec!["b"]);
    assert_eq!(child_names(&children, "a/b"), vec!["c.txt"]);
}

#[test]
fn explicit_dir_entry_upgrades_the_implied_one() {
    // File first (implies `docs/`), then the explicit `docs/` entry: the
    // explicit timestamp must win, and `docs/` must not be duplicated.
    let (nodes, children) = build(vec![file_seed("docs/readme.md", 20), dir_seed("docs")]);

    assert!(nodes["docs"].is_dir);
    assert_eq!(nodes["docs"].modified, Some(1_700_000_500));
    // One entry for `docs` under root, not two.
    assert_eq!(child_names(&children, ""), vec!["docs"]);
}

#[test]
fn explicit_dir_before_its_child_is_not_duplicated() {
    // Order-independence: explicit dir first, then a file inside it.
    let (nodes, children) = build(vec![dir_seed("pics"), file_seed("pics/a.png", 5)]);
    assert!(nodes["pics"].is_dir);
    assert_eq!(child_names(&children, ""), vec!["pics"]);
    assert_eq!(child_names(&children, "pics"), vec!["a.png"]);
}

#[test]
fn mixed_tree_lists_dirs_before_files_alphabetically() {
    let (_nodes, children) = build(vec![
        file_seed("zeta.txt", 1),
        file_seed("alpha.txt", 1),
        dir_seed("mid"),
        file_seed("mid/inner.txt", 1),
    ]);
    // Directories first (mid), then files alpha (alpha.txt, zeta.txt).
    assert_eq!(child_names(&children, ""), vec!["mid", "alpha.txt", "zeta.txt"]);
}

#[test]
fn deeply_nested_single_file_creates_the_whole_chain() {
    let (nodes, _children) = build(vec![file_seed("x/y/z/w/deep.bin", 3)]);
    for dir in ["x", "x/y", "x/y/z", "x/y/z/w"] {
        assert!(nodes.get(dir).is_some_and(|n| n.is_dir), "{dir} should exist");
    }
    assert_eq!(nodes["x/y/z/w/deep.bin"].size, Some(3));
}

#[test]
fn root_node_always_exists_and_is_a_directory() {
    let (nodes, _children) = build(vec![]);
    assert!(nodes[""].is_dir);
    assert_eq!(nodes[""].path, "");
}

#[test]
fn leaf_and_parent_helpers() {
    assert_eq!(leaf_name("a/b/c.txt"), "c.txt");
    assert_eq!(leaf_name("top"), "top");
    assert_eq!(parent_path("a/b/c.txt"), "a/b");
    assert_eq!(parent_path("top"), "");
}

#[test]
fn tree_building_fails_when_node_count_exceeds_the_cap() {
    // Six single-node files plus the root is 7 nodes; a cap of 5 must fail.
    let seeds: Vec<NodeSeed> = (0..6).map(|i| file_seed(&format!("f{i}.txt"), 1)).collect();
    let err = build_tree(seeds.clone(), 5).unwrap_err();
    assert!(matches!(err, ArchiveError::TooLarge(_)), "got {err:?}");
    // The same seeds fit comfortably under a generous cap.
    assert!(build_tree(seeds, 1_000).is_ok());
}

#[test]
fn file_shadowing_a_directory_path_is_first_writer_wins_both_orders() {
    // File `foo` first, then `foo/bar`: `foo` stays a file; `foo/bar` can't
    // live under a file, so it's dropped (not left as an unreachable orphan).
    let (nodes, children) = build(vec![file_seed("foo", 1), file_seed("foo/bar", 2)]);
    assert!(!nodes["foo"].is_dir, "first writer (the file) keeps the path");
    assert!(
        !nodes.contains_key("foo/bar"),
        "the shadowed child is dropped, not orphaned"
    );
    assert_eq!(child_names(&children, ""), vec!["foo"]);

    // Reverse order: `foo/bar` first implies dir `foo`; the later file `foo`
    // yields to the directory (which has children).
    let (nodes, children) = build(vec![file_seed("foo/bar", 2), file_seed("foo", 1)]);
    assert!(nodes["foo"].is_dir, "first writer (the implied dir) keeps the path");
    assert!(nodes.contains_key("foo/bar"));
    assert_eq!(child_names(&children, "foo"), vec!["bar"]);
}
