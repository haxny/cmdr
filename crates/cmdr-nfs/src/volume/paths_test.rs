use super::*;

#[test]
fn a_relative_link_inside_the_export_resolves_against_its_folder() {
    assert_eq!(
        follow_link(&["photos"], b"2024 summer/beach.txt", &[]),
        Some("/photos/2024 summer/beach.txt".to_string())
    );
    assert_eq!(
        follow_link(&[], b"photos", &["2024 summer"]),
        Some("/photos/2024 summer".to_string())
    );
    assert_eq!(
        follow_link(&["a", "b"], b"../c/./d", &["e"]),
        Some("/a/c/d/e".to_string())
    );
    assert_eq!(follow_link(&["a"], b"..", &[]), Some("/".to_string()));
}

#[test]
fn a_link_that_leaves_the_export_or_names_the_servers_tree_is_not_followed() {
    assert_eq!(follow_link(&[], b"/etc/hostname", &[]), None);
    assert_eq!(follow_link(&["a"], b"../../etc", &[]), None);
    assert_eq!(follow_link(&[], b"..", &[]), None);
    assert_eq!(follow_link(&[], b"", &[]), None);
    assert_eq!(follow_link(&[], b"caf\xE9", &[]), None);
}

#[test]
fn paths_split_and_join_by_component() {
    assert_eq!(components("/").count(), 0);
    assert_eq!(components("/a/b").collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!(path_of(&[]), "/");
    assert_eq!(path_of(&["a", "b c"]), "/a/b c");
    assert_eq!(child_of("/", "x"), "/x");
    assert_eq!(child_of("/a", "x"), "/a/x");
}
