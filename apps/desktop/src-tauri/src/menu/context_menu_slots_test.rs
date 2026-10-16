//! Tests for finding a run of rows in the live menu by title (`context_menu_slots.rs`).

use super::*;

fn titles(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_run_is_found_where_it_starts() {
    let menu = ["Open with", "Photos", "Preview", "", "Other…"];
    assert_eq!(find_title_run(&menu, &titles(&["Photos", "Preview"])), Some(1));
    assert_eq!(find_title_run(&menu, &titles(&["Other…"])), Some(4));
}

/// The header line carries the bare filename, so a folder named `Mail` sits above the
/// real `Mail` item. A run of two can't start on it.
#[test]
fn a_lookalike_title_outside_the_run_doesnt_take_it() {
    let menu = ["Mail", "", "AirDrop", "Mail", "Messages"];
    assert_eq!(find_title_run(&menu, &titles(&["Mail", "Messages"])), Some(3));
}

/// Two share extensions may carry one name; inside the run they pair up by position.
#[test]
fn duplicate_titles_inside_a_run_match_by_position() {
    let menu = ["Notes", "Notes", "Reminders"];
    assert_eq!(
        find_title_run(&menu, &titles(&["Notes", "Notes", "Reminders"])),
        Some(0)
    );
}

#[test]
fn a_partial_or_absent_run_matches_nothing() {
    let menu = ["AirDrop", "", "Mail"];
    assert_eq!(find_title_run(&menu, &titles(&["AirDrop", "Mail"])), None);
    assert_eq!(find_title_run(&menu, &titles(&["Messages"])), None);
    assert_eq!(find_title_run(&menu[..1], &titles(&["AirDrop", "Mail"])), None);
}

/// `display_accelerators.rs` puts the glyph after a TAB in the title AppKit reports.
#[test]
fn a_title_matches_without_its_displayed_shortcut() {
    let menu = ["Invert selection\t⇧8", "Select files…\t+"];
    assert_eq!(
        find_title_run(&menu, &titles(&["Invert selection", "Select files…"])),
        Some(0)
    );
}

/// A separator's title is empty, so an empty title in a run would match one.
#[test]
fn an_empty_run_or_an_empty_title_matches_nothing() {
    let menu = ["AirDrop", "", "Mail"];
    assert_eq!(find_title_run(&menu, &[]), None);
    assert_eq!(find_title_run(&menu, &titles(&[""])), None);
    assert_eq!(find_title_run(&menu, &titles(&["AirDrop", ""])), None);
}
