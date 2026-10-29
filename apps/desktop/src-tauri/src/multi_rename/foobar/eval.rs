//! Renders a parsed foobar pattern for one row. Every part evaluates to text plus foobar's truth
//! value: a field with a value is true, literal text is false, a sequence is true when any part
//! is, `[ ]` is its content when true and nothing otherwise. `$if`, `$if2`, `$if3` test truth and
//! return the branch they pick (text and truth); every other function returns the truth of its
//! first argument, so `[$num(%disc%,1)]` drops when the disc is missing.

use super::{Function, MAX_TEXT_BYTES, MAX_WIDTH, Node};
use crate::multi_rename::mask::RowFacts;

pub(super) fn render(nodes: &[Node], row: &RowFacts) -> String {
    sequence(nodes, row).text
}

/// `$num`'s formatting, shared with `%tracknumber%`: `value` in decimal, zero-padded on the left
/// to `width` characters, the minus sign counting. A width under 1 (or 2, negative) pads nothing.
pub(super) fn zero_padded(value: i64, width: usize) -> String {
    let digits = value.unsigned_abs().to_string();
    let sign = if value < 0 { "-" } else { "" };
    let zeros = width.min(MAX_WIDTH).saturating_sub(sign.len() + digits.len());
    format!("{sign}{}{digits}", "0".repeat(zeros))
}

/// Text and its truth.
#[derive(Debug, Default)]
struct Value {
    text: String,
    truth: bool,
}

impl Value {
    fn new(text: String, truth: bool) -> Self {
        Self {
            text: capped(text),
            truth,
        }
    }
}

fn sequence(nodes: &[Node], row: &RowFacts) -> Value {
    let mut all = Value::default();
    for node in nodes {
        let part = node_value(node, row);
        all.text.push_str(&part.text);
        all.text = capped(std::mem::take(&mut all.text));
        all.truth |= part.truth;
    }
    all
}

fn node_value(node: &Node, row: &RowFacts) -> Value {
    match node {
        Node::Literal(text) => Value::new(text.clone(), false),
        Node::Field(field) => field
            .value(row)
            .map_or_else(Value::default, |text| Value::new(text, true)),
        Node::Optional(inner) => {
            let value = sequence(inner, row);
            if value.truth { value } else { Value::default() }
        }
        Node::Call { function, args } => call(*function, args, row),
    }
}

/// Evaluates argument `index` (an empty false value past the end; the parser checked the count).
fn arg(args: &[Vec<Node>], index: usize, row: &RowFacts) -> Value {
    args.get(index)
        .map_or_else(Value::default, |nodes| sequence(nodes, row))
}

fn call(function: Function, args: &[Vec<Node>], row: &RowFacts) -> Value {
    let count = |index| width(leading_int(&arg(args, index, row).text));
    // Every non-control function: its first argument, changed, with that argument's truth.
    let change = |change: &dyn Fn(String) -> String| {
        let subject = arg(args, 0, row);
        Value::new(change(subject.text), subject.truth)
    };
    match function {
        Function::If => {
            let branch = if arg(args, 0, row).truth { 1 } else { 2 };
            arg(args, branch, row)
        }
        Function::If2 => {
            let first = arg(args, 0, row);
            if first.truth { first } else { arg(args, 1, row) }
        }
        Function::If3 => {
            let last = args.len().saturating_sub(1);
            (0..last)
                .map(|index| arg(args, index, row))
                .find(|value| value.truth)
                .unwrap_or_else(|| arg(args, last, row))
        }
        Function::Num => change(&|text| zero_padded(leading_int(&text), count(1))),
        Function::Left => change(&|text| text.chars().take(count(1)).collect()),
        Function::Right => change(&|text| right(&text, count(1))),
        Function::Replace => change(&|text| replaced(text, args, row)),
        Function::Lower => change(&|text| text.to_lowercase()),
        Function::Upper => change(&|text| text.to_uppercase()),
        Function::Caps => change(&|text| caps(&text)),
        Function::Trim => change(&|text| text.trim_matches(' ').to_string()),
        Function::Pad => change(&|text| padded(text, count(1), &arg(args, 2, row).text)),
    }
}

/// A count from a pattern: negative is 0, and nothing is wider than [`MAX_WIDTH`].
fn width(number: i64) -> usize {
    usize::try_from(number.max(0)).map_or(MAX_WIDTH, |n| n.min(MAX_WIDTH))
}

/// The integer `text` starts with, as foobar reads numbers: an optional sign and digits
/// (`12x` → 12, `-4.8` → -4), 0 when there are none; saturating, never overflowing.
fn leading_int(text: &str) -> i64 {
    let text = text.trim_start();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let magnitude = digits
        .chars()
        .map_while(|c| c.to_digit(10))
        .fold(0_i64, |n, digit| n.saturating_mul(10).saturating_add(i64::from(digit)));
    if negative { -magnitude } else { magnitude }
}

/// The last `count` characters.
fn right(text: &str, count: usize) -> String {
    let skip = text.chars().count().saturating_sub(count);
    text.chars().skip(skip).collect()
}

/// `$replace(text,from1,to1,from2,to2,…)`: each pair in turn, over the result of the previous;
/// an empty `from` replaces nothing.
fn replaced(text: String, args: &[Vec<Node>], row: &RowFacts) -> String {
    (1..args.len()).step_by(2).fold(text, |text, index| {
        let from = arg(args, index, row).text;
        if from.is_empty() {
            return text;
        }
        let to = arg(args, index + 1, row).text;
        let mut out = String::new();
        let mut rest = text.as_str();
        while let Some(found) = rest.find(&from) {
            out.push_str(&rest[..found]);
            out.push_str(&to);
            rest = &rest[found + from.len()..];
            if out.len() >= MAX_TEXT_BYTES {
                return capped(out);
            }
        }
        out.push_str(rest);
        capped(out)
    })
}

/// The characters after which `$caps` starts a word: whitespace, and an opening bracket or quote.
fn starts_word_after(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '/')
}

/// Every word's first letter upper case, the rest lower case.
fn caps(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    for c in text.chars() {
        if word_start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        word_start = starts_word_after(c);
    }
    out
}

/// `text` padded on the right to `width` characters with `fill`'s first character (a space
/// when `fill` is empty).
fn padded(mut text: String, width: usize, fill: &str) -> String {
    let fill = fill.chars().next().unwrap_or(' ');
    let missing = width.saturating_sub(text.chars().count());
    text.extend(std::iter::repeat_n(fill, missing));
    text
}

/// `text` cut to [`MAX_TEXT_BYTES`] at a character boundary.
fn capped(mut text: String) -> String {
    if text.len() > MAX_TEXT_BYTES {
        let end = text.floor_char_boundary(MAX_TEXT_BYTES);
        text.truncate(end);
    }
    text
}
