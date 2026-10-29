//! foobar2000 title formatting for the name mask: the user pastes a foobar pattern and it renders
//! as foobar's File Operations would. A pure subset, parsed once and rendered per row.
//!
//! - `%field%` a field (case-insensitive; inner spaces count: `%album artist%`). The fields and
//!   their fallbacks are in `fields.rs`. Outside `[ ]`, a field with no value is no text (foobar
//!   shows `?`; File Operations, which renames, shows nothing).
//! - `[ … ]` kept only when something inside is true (a field with a value, or a function whose
//!   result is true), else dropped whole.
//! - `'…'` literal text (`%`, `$`, `[`, `(`, `,` all plain inside); `''` is one apostrophe.
//! - `$num $if $if2 $if3 $left $right $replace $lower $upper $caps $trim $pad $year`, in
//!   `eval.rs`. Names are case-insensitive. Inside an argument `,` and `)` end it, except within a
//!   balanced `( )` of the argument's own text (`$if(%date%,x (y),z)`); quote anything else.
//! - Anything else is a [`FoobarError`] naming it and its byte offset.
//!
//! Nesting (brackets and calls together) stops at [`MAX_DEPTH`], every value at
//! [`MAX_TEXT_BYTES`], and a `$num` / `$pad` width at [`MAX_WIDTH`], so no input panics, overflows
//! the stack, or allocates without bound.

mod eval;
mod fields;
mod parse;

use cmdr_media_meta::MediaField;
use fields::Field;

use super::mask::RowFacts;

/// The deepest nesting of `[ ]` and `$f( )` together.
pub const MAX_DEPTH: usize = 32;

/// The longest value any part of a pattern renders to, in bytes (a name is at most 255).
pub const MAX_TEXT_BYTES: usize = 4096;

/// The widest `$num` or `$pad`, in characters.
pub const MAX_WIDTH: usize = 255;

/// Why a foobar pattern doesn't parse. Typed, so the sheet words it; `at` is a byte offset into
/// the pattern.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FoobarError {
    /// `%name%` names no field Cmdr knows; `name` as typed.
    UnknownField { name: String, at: usize },
    /// `$name(` names no function Cmdr knows; `name` as typed (empty for a bare `$`).
    UnknownFunction { name: String, at: usize },
    /// A known function with the wrong number of arguments, or none at all (`$upper`).
    BadArguments { function: String, at: usize },
    /// An opening `%`, `[`, `'`, `$f(`, or a `(` in an argument's text with no end.
    Unclosed { what: Unclosed, at: usize },
    /// Nested deeper than [`MAX_DEPTH`]; `at` is the first opening past it.
    TooDeep { at: usize },
}

/// What a [`FoobarError::Unclosed`] left open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Unclosed {
    Field,
    Bracket,
    Quote,
    Function,
    /// A `(` in a function argument's text (`$upper(a (b`).
    Parenthesis,
}

/// A parsed pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(Vec<Node>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Literal(String),
    Field(Field),
    /// `[ … ]`.
    Optional(Vec<Node>),
    Call {
        function: Function,
        args: Vec<Vec<Node>>,
    },
}

/// The functions Cmdr evaluates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Function {
    Num,
    If,
    If2,
    If3,
    Left,
    Right,
    Replace,
    Lower,
    Upper,
    Caps,
    Trim,
    Pad,
    Year,
}

impl Function {
    fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "num" => Self::Num,
            "if" => Self::If,
            "if2" => Self::If2,
            "if3" => Self::If3,
            "left" => Self::Left,
            "right" => Self::Right,
            "replace" => Self::Replace,
            "lower" => Self::Lower,
            "upper" => Self::Upper,
            "caps" => Self::Caps,
            "trim" => Self::Trim,
            "pad" => Self::Pad,
            "year" => Self::Year,
            _ => return None,
        })
    }

    /// Whether `count` arguments is a call foobar accepts.
    fn takes(self, count: usize) -> bool {
        match self {
            Self::Lower | Self::Upper | Self::Caps | Self::Trim | Self::Year => count == 1,
            Self::Num | Self::Left | Self::Right | Self::If2 => count == 2,
            Self::If | Self::Pad => (2..=3).contains(&count),
            Self::If3 => count >= 2,
            Self::Replace => count >= 3 && count % 2 == 1,
        }
    }
}

/// Parses `pattern` once; render it per row with [`Pattern::render`].
pub fn parse(pattern: &str) -> Result<Pattern, FoobarError> {
    parse::parse(pattern).map(Pattern)
}

impl Pattern {
    /// The name this pattern makes for `facts`. Never fails: a missing value is no text.
    pub fn render(&self, facts: &RowFacts) -> String {
        eval::render(&self.0, facts)
    }

    /// Whether any field comes from the file's tags (so the row waits for them).
    pub fn uses_media(&self) -> bool {
        self.media_field().is_some()
    }

    /// The first tag field the pattern names, in reading order: what a row whose tags can't be
    /// read reports as unknown. `None` when every field comes from the file's name or folder.
    pub fn media_field(&self) -> Option<MediaField> {
        fn first(nodes: &[Node]) -> Option<MediaField> {
            nodes.iter().find_map(|node| match node {
                Node::Literal(_) => None,
                Node::Field(field) => field.media_field(),
                Node::Optional(inner) => first(inner),
                Node::Call { args, .. } => args.iter().find_map(|arg| first(arg)),
            })
        }
        first(&self.0)
    }
}
