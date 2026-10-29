//! Tests for the Disc tools.

use super::disc::*;
use super::model::{FileTags, TagField};
use super::tools::{TagTool, ToolContext, apply_tool_in};

fn with_disc(value: Option<&str>) -> FileTags {
    let mut tags = FileTags::default();
    if let Some(value) = value {
        tags.fields.insert(TagField::Disc, vec![value.to_string()]);
    }
    tags
}

fn discs(files: &[FileTags]) -> Vec<Option<String>> {
    files
        .iter()
        .map(|f| f.fields.get(&TagField::Disc).and_then(|v| v.first()).cloned())
        .collect()
}

fn ctx(folders: &[&str]) -> ToolContext {
    ToolContext {
        folders: folders.iter().map(|f| (*f).to_string()).collect(),
        ..ToolContext::default()
    }
}

fn some(values: &[&str]) -> Vec<Option<String>> {
    values.iter().map(|v| Some((*v).to_string())).collect()
}

#[test]
fn folder_names_with_cd_disc_disk_give_their_number() {
    let folders = ["/b/CD 1", "/b/CD2", "/b/Disc 03", "/b/Disk 4", "/b/cd-5"];
    let files = vec![with_disc(None); folders.len()];
    let out = disc_from_folder(&files, &ctx(&folders));
    assert_eq!(discs(&out), some(&["1", "2", "3", "4", "5"]));
}

#[test]
fn other_folders_get_their_order_by_natural_collation() {
    let folders = ["/b/Druhá", "/b/Druhá", "/b/Čtvrtá", "/b/Alfa", "/b/Alfa"];
    let files = vec![with_disc(Some("x")); folders.len()];
    let out = disc_from_folder(&files, &ctx(&folders));
    // Alfa, Čtvrtá, Druhá: diacritics are folded for the order.
    assert_eq!(discs(&out), some(&["3", "3", "2", "1", "1"]));
}

#[test]
fn natural_order_puts_ten_after_two() {
    let folders = ["/b/Part 10", "/b/Part 2"];
    let out = disc_from_folder(&vec![with_disc(None); 2], &ctx(&folders));
    assert_eq!(discs(&out), some(&["2", "1"]));
}

#[test]
fn one_plain_folder_leaves_the_discs_alone() {
    let folders = ["/b/Kniha", "/b/Kniha"];
    let files = vec![with_disc(Some("CD1")), with_disc(None)];
    let out = disc_from_folder(&files, &ctx(&folders));
    assert_eq!(discs(&out), vec![Some("CD1".to_string()), None]);
}

#[test]
fn one_folder_named_for_a_disc_still_sets_it() {
    let out = disc_from_folder(&[with_disc(None)], &ctx(&["/b/CD 2"]));
    assert_eq!(discs(&out), some(&["2"]));
}

#[test]
fn number_only_cuts_the_word_and_keeps_a_total() {
    let files = [
        with_disc(Some("CD2")),
        with_disc(Some("Disk 10/12")),
        with_disc(Some("3")),
        with_disc(Some("1/2")),
        with_disc(Some("bonus")),
        with_disc(None),
    ];
    let out = disc_number_only(&files);
    assert_eq!(
        discs(&out),
        vec![
            Some("2".into()),
            Some("10/12".into()),
            Some("3".into()),
            Some("1/2".into()),
            Some("bonus".into()),
            None
        ]
    );
}

#[test]
fn the_tools_run_through_the_tool_enum() {
    let files = [with_disc(Some("CD2"))];
    let out = apply_tool_in(&files, &ToolContext::default(), &TagTool::DiscNumberOnly);
    assert_eq!(discs(&out), some(&["2"]));
    let out = apply_tool_in(&[with_disc(None)], &ctx(&["/b/Disc 7"]), &TagTool::DiscFromFolder);
    assert_eq!(discs(&out), some(&["7"]));
}

#[test]
fn unnamed_folders_follow_the_highest_named_disc() {
    let folders = ["/b/Bonus", "/b/CD 1", "/b/CD 2", "/b/Extra"];
    let out = disc_from_folder(&vec![with_disc(None); 4], &ctx(&folders));
    assert_eq!(discs(&out), some(&["3", "1", "2", "4"]));
}

#[test]
fn a_selection_keeps_the_book_wide_number_of_an_unnamed_folder() {
    let book = ["/b/Part A", "/b/Part B", "/b/Part C"];
    let selected = ToolContext {
        folders: vec!["/b/Part B".to_string()],
        book_folders: book.iter().map(|f| (*f).to_string()).collect(),
        ..ToolContext::default()
    };
    let out = disc_from_folder(&[with_disc(None)], &selected);
    assert_eq!(discs(&out), some(&["2"]));
}

#[test]
fn a_selection_of_a_flat_book_stays_alone() {
    let selected = ToolContext {
        folders: vec!["/b/Kniha".to_string()],
        book_folders: vec!["/b/Kniha".to_string(), "/b/Kniha".to_string()],
        ..ToolContext::default()
    };
    let out = disc_from_folder(&[with_disc(None)], &selected);
    assert_eq!(discs(&out), vec![None]);
}
