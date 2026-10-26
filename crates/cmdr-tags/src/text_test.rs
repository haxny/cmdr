//! Tests for the diacritics and case helpers.

use super::text::{remove_diacritics, sentence_case, title_case};

#[test]
fn remove_diacritics_strips_latin_marks_and_maps_undecomposed_letters() {
    assert_eq!(remove_diacritics("Zahálka"), "Zahalka");
    assert_eq!(remove_diacritics("Łódź"), "Lodz");
}

#[test]
fn remove_diacritics_leaves_other_scripts_alone() {
    assert_eq!(remove_diacritics("й"), "й");
}

#[test]
fn sentence_case_lowercases_and_uppercases_the_first_letter() {
    assert_eq!(sentence_case("Laik Na Kolobezce"), "Laik na kolobezce");
    assert_eq!(sentence_case("JEFFREY ARCHER"), "Jeffrey archer");
    assert_eq!(sentence_case("ŽLUŤOUČKÝ KŮŇ"), "Žluťoučký kůň");
}

#[test]
fn sentence_case_edge_cases() {
    assert_eq!(sentence_case(""), "");
    assert_eq!(sentence_case("12 opic"), "12 opic");
    assert_eq!(sentence_case("„Ahoj"), "„Ahoj", "first letter, not first char");
}

#[test]
fn title_case_capitalizes_each_space_separated_word() {
    assert_eq!(title_case("jeffrey archer"), "Jeffrey Archer");
    assert_eq!(title_case("JEFFREY  ARCHER"), "Jeffrey  Archer");
}
