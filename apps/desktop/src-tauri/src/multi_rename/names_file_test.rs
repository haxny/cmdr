//! The Results file: what it holds, and reading the user's edits back.

use super::names_file::{NameEdit, parse, render, write};
use super::plan::{PreviewRow, RowStatus};
use crate::test_support::TestDir;

fn row(old: &str, new: &str) -> PreviewRow {
    PreviewRow {
        row: 0,
        old_name: old.to_string(),
        new_name: new.to_string(),
        status: RowStatus::Ready,
    }
}

fn edit(old: &str, new: &str) -> NameEdit {
    NameEdit {
        old_name: old.to_string(),
        new_name: new.to_string(),
    }
}

#[test]
fn one_line_per_row_old_then_new() {
    assert_eq!(
        render(&[row("Žádost.pdf", "Zadost.pdf"), row("b.txt", "b.txt")]),
        "Žádost.pdf\tZadost.pdf\nb.txt\tb.txt\n"
    );
}

#[test]
fn reading_back_is_the_render_inverted() {
    let rows = [row("Žádost.pdf", "Zadost.pdf"), row("with space.txt", "x.txt")];
    assert_eq!(
        parse(&render(&rows)),
        vec![edit("Žádost.pdf", "Zadost.pdf"), edit("with space.txt", "x.txt")]
    );
}

#[test]
fn an_editor_s_line_endings_and_blanks_are_tolerated() {
    assert_eq!(
        parse("a.txt\t  new a.txt  \r\nb.txt\tb2.txt\r\n\n"),
        vec![edit("a.txt", "new a.txt"), edit("b.txt", "b2.txt")]
    );
}

#[test]
fn the_new_name_is_after_the_last_tab_and_a_line_without_one_is_skipped() {
    assert_eq!(
        parse("odd\tname.txt\tfixed.txt\nno tab here\n\tno old name\n"),
        vec![edit("odd\tname.txt", "fixed.txt")]
    );
}

#[test]
fn write_puts_the_rows_in_a_text_file() {
    let dir = TestDir::new("multi-rename-names-file");
    let path = write(&dir, &[row("a.txt", "b.txt")]).expect("the scratch dir is writable");
    assert_eq!(std::fs::read_to_string(path).expect("the file reads"), "a.txt\tb.txt\n");
}
