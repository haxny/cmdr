//! Text helpers shared by the tag editor and Multi-Rename: diacritics removal and
//! sentence/title case.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// `text` without diacritics on Latin and Greek letters: each such letter is
/// decomposed (NFD) and its combining marks dropped, plus a table for the letters
/// that don't decompose (`ł`, `đ`, `ø`, `ß`, `æ`, `œ`, `þ`, `ı`), like foobar2000's
/// `$ascii()`. Every other script is left exactly as it is: a kana's dakuten, a
/// Devanagari or Thai vowel sign, and Cyrillic `й` / `ё` are part of the letter,
/// not decoration.
pub fn remove_diacritics(text: &str) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.nfc() {
        if let Some(replacement) = undecomposed(c) {
            out.push_str(replacement);
        } else if is_latin_or_greek(c) {
            out.extend(c.to_string().nfd().filter(|m| !is_combining_mark(*m)));
        } else {
            out.push(c);
        }
    }
    out
}

/// Latin (Basic through Extended Additional) and Greek letters: the scripts whose
/// marks are diacritics a name can do without.
fn is_latin_or_greek(c: char) -> bool {
    matches!(c as u32, 0x00C0..=0x024F | 0x1E00..=0x1EFF | 0x0370..=0x03FF | 0x1F00..=0x1FFF)
}

fn undecomposed(c: char) -> Option<&'static str> {
    Some(match c {
        'ł' => "l",
        'Ł' => "L",
        'đ' => "d",
        'Đ' => "D",
        'ø' => "o",
        'Ø' => "O",
        'ß' => "ss",
        'ẞ' => "SS",
        'æ' => "ae",
        'Æ' => "Ae",
        'œ' => "oe",
        'Œ' => "Oe",
        'þ' => "th",
        'Þ' => "Th",
        'ð' => "d",
        'Ð' => "D",
        'ı' => "i",
        'ħ' => "h",
        'Ħ' => "H",
        'ŧ' => "t",
        'Ŧ' => "T",
        _ => return None,
    })
}

/// `text` lowercased with its first letter uppercased. The first *letter*, not the
/// first char: a leading quote or other punctuation is skipped (`„ahoj` becomes
/// `„Ahoj`), but a leading digit ends the search, so `12 opic` stays as it is.
pub fn sentence_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut seeking = true;
    for c in text.chars() {
        if seeking && c.is_alphanumeric() {
            seeking = false;
            if c.is_alphabetic() {
                out.extend(c.to_uppercase());
                continue;
            }
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// `text` with the first letter of each space-separated word uppercased and the
/// rest lowercased. Only spaces split words, so `o'neil` becomes `O'neil`.
pub fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut starts_word = true;
    for c in text.chars() {
        if starts_word && c.is_alphabetic() {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        starts_word = c == ' ' || (starts_word && !c.is_alphabetic());
    }
    out
}
