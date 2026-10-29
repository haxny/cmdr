//! The foobar pattern parser: one pass, recursive only into `[ ]` and `$f( )`, and only
//! [`MAX_DEPTH`] deep, so a pathological pattern is an error, never a stack overflow.

use super::fields::Field;
use super::{FoobarError, Function, MAX_DEPTH, Node, Unclosed};

pub(super) fn parse(pattern: &str) -> Result<Vec<Node>, FoobarError> {
    let mut parser = Parser { src: pattern, pos: 0 };
    // At the top level nothing ends a sequence but the end of the pattern.
    let (nodes, _) = parser.sequence(Context::Top, 0)?;
    Ok(nodes)
}

/// Where a sequence sits, which decides the characters that end it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Top,
    /// Inside `[ ]`: `]` ends it.
    Optional,
    /// A function argument: `,` and `)` end it.
    Argument,
}

/// What ended a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Pattern,
    Bracket,
    Comma,
    Paren,
}

struct Parser<'a> {
    src: &'a str,
    /// A byte offset, always on a char boundary.
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    /// Text, fields, brackets, and calls up to whatever ends `context`.
    fn sequence(&mut self, context: Context, depth: usize) -> Result<(Vec<Node>, End), FoobarError> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        let end = loop {
            let Some(c) = self.peek() else { break End::Pattern };
            let at = self.pos;
            let closing = match (c, context) {
                (']', Context::Optional) => Some(End::Bracket),
                (',', Context::Argument) => Some(End::Comma),
                (')', Context::Argument) => Some(End::Paren),
                _ => None,
            };
            if let Some(end) = closing {
                self.pos += 1;
                break end;
            }
            match c {
                '\'' => text.push_str(self.quoted(at)?),
                '%' | '[' | '$' => {
                    flush(&mut nodes, &mut text);
                    nodes.push(match c {
                        '%' => Node::Field(self.field(at)?),
                        '[' => self.optional(at, depth)?,
                        _ => self.call(at, depth)?,
                    });
                }
                _ => {
                    text.push(c);
                    self.pos += c.len_utf8();
                }
            }
        };
        flush(&mut nodes, &mut text);
        Ok((nodes, end))
    }

    /// `'…'` from the quote at `at`: the text up to the next quote; `''` is one apostrophe.
    fn quoted(&mut self, at: usize) -> Result<&str, FoobarError> {
        let body = at + 1;
        let close = self.src[body..].find('\'').ok_or(FoobarError::Unclosed {
            what: Unclosed::Quote,
            at,
        })?;
        self.pos = body + close + 1;
        Ok(if close == 0 { "'" } else { &self.src[body..body + close] })
    }

    /// `%name%` from the `%` at `at`.
    fn field(&mut self, at: usize) -> Result<Field, FoobarError> {
        let body = at + 1;
        let close = self.src[body..].find('%').ok_or(FoobarError::Unclosed {
            what: Unclosed::Field,
            at,
        })?;
        let name = &self.src[body..body + close];
        self.pos = body + close + 1;
        Field::parse(name).ok_or_else(|| FoobarError::UnknownField {
            name: name.to_string(),
            at,
        })
    }

    /// `[ … ]` from the `[` at `at`.
    fn optional(&mut self, at: usize, depth: usize) -> Result<Node, FoobarError> {
        if depth >= MAX_DEPTH {
            return Err(FoobarError::TooDeep { at });
        }
        self.pos = at + 1;
        match self.sequence(Context::Optional, depth + 1)? {
            (inner, End::Bracket) => Ok(Node::Optional(inner)),
            _ => Err(FoobarError::Unclosed {
                what: Unclosed::Bracket,
                at,
            }),
        }
    }

    /// `$name(arg,…)` from the `$` at `at`.
    fn call(&mut self, at: usize, depth: usize) -> Result<Node, FoobarError> {
        let body = at + 1;
        let name_len = self.src[body..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(self.src.len() - body);
        let name = &self.src[body..body + name_len];
        self.pos = body + name_len;
        let function = Function::parse(name).ok_or_else(|| FoobarError::UnknownFunction {
            name: name.to_string(),
            at,
        })?;
        let bad = || FoobarError::BadArguments {
            function: name.to_ascii_lowercase(),
            at,
        };
        if self.peek() != Some('(') {
            return Err(bad());
        }
        if depth >= MAX_DEPTH {
            return Err(FoobarError::TooDeep { at });
        }
        self.pos += 1;
        let mut args = Vec::new();
        if self.peek() == Some(')') {
            self.pos += 1;
        } else {
            loop {
                let (arg, end) = self.sequence(Context::Argument, depth + 1)?;
                args.push(arg);
                match end {
                    End::Comma => {}
                    End::Paren => break,
                    End::Pattern | End::Bracket => {
                        return Err(FoobarError::Unclosed {
                            what: Unclosed::Function,
                            at,
                        });
                    }
                }
            }
        }
        if !function.takes(args.len()) {
            return Err(bad());
        }
        Ok(Node::Call { function, args })
    }
}

/// Moves the pending literal `text`, if any, into `nodes`.
fn flush(nodes: &mut Vec<Node>, text: &mut String) {
    if !text.is_empty() {
        nodes.push(Node::Literal(std::mem::take(text)));
    }
}
