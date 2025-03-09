//! Shell glob-like filename matching.
//!
//! The glob-style pattern features currently supported are:
//! - any character except `?`, `*`, `[`, `\`, or `{` is matched literally
//! - `?` matches any single character except a slash (`/`)
//! - `*` matches any sequence of zero or more characters that does not
//!   contain a slash (`/`)
//! - a backslash allows the next character to be matched literally, except
//!   for the `\a`, `\b`, `\e`, `\n`, `\r`, and `\v` sequences
//! - a `[...]` character class supports ranges, negation if the very first
//!   character is `!`, backslash-escaping, and also matching
//!   a `]` character if it is the very first character possibly after
//!   the `!` one (e.g. `[]]` would only match a single `]` character)
//! - an `{a,bbb,cc}` alternation supports backslash-escaping, but not
//!   nested alternations or character classes yet
//!
//! Note that the `*` and `?` wildcard patterns, as well as the character
//! classes, will never match a slash.
//!
//! Examples:
//! - `abc.txt` would only match `abc.txt`
//! - `foo/test?.txt` would match e.g. `foo/test1.txt` or `foo/test".txt`,
//!   but not `foo/test/.txt`
//! - `/etc/c[--9].conf` would match e.g. `/etc/c-.conf`, `/etc/c..conf`,
//!   or `/etc/7.conf`, but not `/etc/c/.conf`
//! - `linux-[0-9]*-{generic,aws}` would match `linux-5.2.27b1-generic`
//!   and `linux-4.0.12-aws`, but not `linux-unsigned-5.2.27b1-generic`
//!
//! Note that the [`glob_to_regex`] function returns a regular expression
//! that will only verify whether a specified text string matches
//! the pattern; it does not in any way attempt to look up any paths on
//! the filesystem.
//!
//! ```rust
//! # use std::error::Error;
//!
//! # fn main() -> Result<(), Box<dyn Error>> {
//! let re_name = fdf::glob::glob_to_regex("linux-[0-9]*-{generic,aws}")?;
//! for name in &[
//!     "linux-5.2.27b1-generic",
//!     "linux-4.0.12-aws",
//!     "linux-unsigned-5.2.27b1-generic"
//! ] {
//!     let okay = re_name.is_match(name);
//!     println!(
//!         "{}: {}",
//!         name,
//!         match okay { true => "yes", false => "no" },
//!     );
//!     assert!(okay == !name.contains("unsigned"));
//! }
//! # Ok(())
//! # }
//! ```

/*
 * Copyright (c) 2021, 2022  Peter Pentchev <roam@ringlet.net>
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */

use std::fmt;
use std::mem;
use std::vec::IntoIter as VecIntoIter;

use regex::Regex;

/// Error type for glob pattern operations
#[derive(Debug)]
pub enum Error {
    /// Bare escape at the end of the pattern
    BareEscape,
    /// An unclosed character class
    UnclosedClass,
    /// A feature that is not yet implemented
    NotImplemented(String),
    /// A range where the start character is after the end one
    ReversedRange(char, char),
    /// A dash after a range in a character class
    RangeAfterRange(char, char),
    /// An unclosed alternation
    UnclosedAlternation,
    /// An invalid regular expression was generated from the pattern
    InvalidRegex(String, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BareEscape => write!(f, "Bare escape at the end of the pattern"),
            Self::UnclosedClass => write!(f, "Unclosed character class"),
            Self::NotImplemented(s) => write!(f, "Not implemented: {s}"),
            Self::ReversedRange(start, end) => {
                write!(f, "Reversed range: {start} > {end}")
            }
            Self::RangeAfterRange(start, end) => {
                write!(f, "Range after range: {start}-{end}")
            }
            Self::UnclosedAlternation => write!(f, "Unclosed alternation"),
            Self::InvalidRegex(pattern, error) => {
                write!(f, "Invalid regex pattern '{pattern}': {error}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Something that may appear in a character class.
#[derive(Debug)]
enum ClassItem {
    /// A character may appear in a character class.
    Char(char),
    /// A range of characters may appear in a character class.
    Range(char, char),
}

/// An accumulator for building the representation of a character class.
#[derive(Debug)]
struct ClassAccumulator {
    /// Is the class negated (i.e. was `^` the first character).
    negated: bool,
    /// The characters or ranges in the class, in order of appearance.
    items: Vec<ClassItem>,
}

/// The current state of the glob pattern parser.
#[derive(Debug)]
enum State {
    /// The very start of the pattern.
    Start,
    /// The end of the pattern, nothing more to do.
    End,
    /// The next item can be a literal character.
    Literal,
    /// The next item will signify a character escape, e.g. `\t`, `\n`, etc.
    Escape,
    /// The next item will be the first character of a class, possibly `^`.
    ClassStart,
    /// The next item will either be a character or a range, both within a class.
    Class(ClassAccumulator),
    /// A character class range was completed; check whether the next character is
    /// a dash.
    ClassRange(ClassAccumulator, char),
    /// There was a dash following a character range; let's hope this is the end of
    /// the class definition.
    ClassRangeDash(ClassAccumulator),
    /// The next item will signify a character escape within a character class.
    ClassEscape(ClassAccumulator),
    /// We are building a collection of alternatives.
    Alternate(String, Vec<String>),
    /// The next item will signify a character escape within a collection of alternatives.
    AlternateEscape(String, Vec<String>),
}

// We need this so we can use mem::take() later.
impl Default for State {
    fn default() -> Self {
        Self::Start
    }
}

/// Escape a character in a character class if necessary.
/// This only escapes the backslash itself and the closing bracket.
fn escape_in_class(chr: char) -> String {
    if chr == ']' || chr == '\\' {
        format!("\\{chr}")
    } else {
        chr.to_string()
    }
}

/// Escape a character outside of a character class if necessary.
fn escape(chr: char) -> String {
    if "[{(|^$.*?+\\".contains(chr) {
        format!("\\{chr}")
    } else {
        chr.to_string()
    }
}

/// Interpret an escaped character: return the one that was meant.
const fn map_letter_escape(chr: char) -> char {
    match chr {
        'a' => '\x07',
        'b' => '\x08',
        'e' => '\x1b',
        'f' => '\x0c',
        'n' => '\x0a',
        'r' => '\x0d',
        't' => '\x09',
        'v' => '\x0b',
        other => other,
    }
}

/// Unescape a character and escape it if needed.
fn escape_special(chr: char) -> String {
    escape(map_letter_escape(chr))
}

/// Remove a slash from characters and classes.
struct ExcIter<I>
where
    I: Iterator<Item = ClassItem>,
{
    /// The items to remove slashes from.
    it: I,
}

impl<I> Iterator for ExcIter<I>
where
    I: Iterator<Item = ClassItem>,
{
    type Item = VecIntoIter<ClassItem>;

    fn next(&mut self) -> Option<Self::Item> {
        self.it.next().map(|cls| {
            match cls {
                ClassItem::Char('/') => vec![],
                ClassItem::Char(_) => vec![cls],
                ClassItem::Range('.', '/') => vec![ClassItem::Char('.')],
                ClassItem::Range(start, '/') => vec![ClassItem::Range(start, '.')],
                ClassItem::Range('/', '0') => vec![ClassItem::Char('0')],
                ClassItem::Range('/', end) => vec![ClassItem::Range('0', end)],
                ClassItem::Range(start, end) if start > '/' || end < '/' => vec![cls],
                ClassItem::Range(start, end) => vec![
                    if start == '.' {
                        ClassItem::Char('.')
                    } else {
                        ClassItem::Range(start, '.')
                    },
                    if end == '0' {
                        ClassItem::Char('0')
                    } else {
                        ClassItem::Range('0', end)
                    },
                ],
            }
            .into_iter()
        })
    }
}

/// Exclude the slash character from classes that would include it.
fn handle_slash_exclude(acc: ClassAccumulator) -> ClassAccumulator {
    assert!(!acc.negated);
    ClassAccumulator {
        items: ExcIter {
            it: acc.items.into_iter(),
        }
        .flatten()
        .collect(),
        ..acc
    }
}

/// Make sure a character class will match a slash.
fn handle_slash_include(mut acc: ClassAccumulator) -> ClassAccumulator {
    assert!(acc.negated);
    let slash_found = acc.items.iter().any(|item| match *item {
        ClassItem::Char('/') => true,
        ClassItem::Char(_) => false,
        ClassItem::Range(start, end) => start <= '/' && end >= '/',
    });
    if !slash_found {
        acc.items.push(ClassItem::Char('/'));
    }
    acc
}

/// Character classes should never match a slash when used in filenames.
/// Thus, make sure that a negated character class will include the slash
/// character and that a non-negated one will not include it.
fn handle_slash(acc: ClassAccumulator) -> ClassAccumulator {
    if acc.negated {
        handle_slash_include(acc)
    } else {
        handle_slash_exclude(acc)
    }
}

/// Convert a glob character class to a regular expression one.
/// Make sure none of the classes will allow a slash to be matched in
/// a filename, make sure the dash is at the end of the regular expression
/// class pattern (e.g. `[A-Za-z0-9-]`), sort the characters and the classes.
fn close_class(glob_acc: ClassAccumulator) -> String {
    let acc = handle_slash(glob_acc);

    // Partition items into chars and ranges (replace itertools::partition_map)
    let mut chars_vec = Vec::new();
    let mut classes_vec = Vec::new();

    for item in acc.items {
        match item {
            ClassItem::Char(chr) => chars_vec.push(chr),
            ClassItem::Range(start, end) => classes_vec.push((start, end)),
        }
    }

    let (chars, final_dash) = {
        let mut has_dash = false;
        let mut chars_filtered = Vec::new();

        for chr in chars_vec {
            if chr == '-' {
                has_dash = true;
            } else {
                chars_filtered.push(chr);
            }
        }

        // Sort and deduplicate (replace itertools::sorted_unstable and dedup)
        chars_filtered.sort_unstable();
        chars_filtered.dedup();

        let formatted_chars = chars_filtered
            .into_iter()
            .map(escape_in_class)
            .collect::<Vec<_>>();
        (formatted_chars, if has_dash { "-" } else { "" })
    };

    // Sort classes (replace itertools::sorted_unstable and dedup)
    classes_vec.sort_unstable();
    classes_vec.dedup();

    let classes = classes_vec
        .into_iter()
        .map(|cls| format!("{}-{}", escape_in_class(cls.0), escape_in_class(cls.1)))
        .collect::<Vec<_>>();

    // Combine characters and classes
    let mut result = String::new();
    if acc.negated {
        result.push('^');
    }

    for char_str in chars {
        result.push_str(&char_str);
    }

    for class_str in classes {
        result.push_str(&class_str);
    }

    result.push_str(final_dash);

    format!("[{result}]")
}

/// Convert a glob alternatives list to a regular expression pattern.
fn close_alternate(gathered: Vec<String>) -> String {
    // Sort and deduplicate items (replace itertools::sorted_unstable, dedup, join)
    let mut items = gathered
        .into_iter()
        .map(|item| item.chars().map(escape).collect::<String>())
        .collect::<Vec<_>>();

    items.sort_unstable();
    items.dedup();

    let joined = items.join("|");
    format!("({joined})")
}

/// Iterate over a glob pattern's characters, build up a regular expression.
struct GlobIterator<I: Iterator<Item = char>> {
    /// The iterator over the glob pattern's characters.
    pattern: I,
    /// The current state of the glob pattern parser.
    state: State,
}

/// Either a piece of the regular expression or an error.
type StringResult = Result<Option<String>, Error>;

impl<I> GlobIterator<I>
where
    I: Iterator<Item = char>,
{
    /// Output a "^" at the very start of the pattern.
    fn handle_start(&mut self) -> String {
        self.state = State::Literal;
        "^".to_owned()
    }

    /// Handle the next character when expecting a literal one.
    fn handle_literal(&mut self) -> Option<String> {
        match self.pattern.next() {
            None => {
                self.state = State::End;
                Some("$".to_owned())
            }
            Some(chr) => {
                let (new_state, res) = match chr {
                    '\\' => (State::Escape, None),
                    '[' => (State::ClassStart, None),
                    '{' => (State::Alternate(String::new(), Vec::new()), None),
                    '?' => (State::Literal, Some("[^/]".to_owned())),
                    '*' => (State::Literal, Some(".*".to_owned())),
                    ']' | '}' | '.' => (State::Literal, Some(format!("\\{chr}"))),
                    _ => (State::Literal, Some(format!("{chr}"))),
                };
                self.state = new_state;
                res
            }
        }
    }

    /// Handle an escaped character.
    fn handle_escape(&mut self) -> StringResult {
        match self.pattern.next() {
            Some(chr) => {
                self.state = State::Literal;
                Ok(Some(escape_special(chr)))
            }
            None => Err(Error::BareEscape),
        }
    }

    /// Handle the first character in a character class specification.
    fn handle_class_start(&mut self) -> StringResult {
        match self.pattern.next() {
            Some(chr) => {
                self.state = match chr {
                    '!' => State::Class(ClassAccumulator {
                        negated: true,
                        items: Vec::new(),
                    }),
                    '-' => State::Class(ClassAccumulator {
                        negated: false,
                        items: vec![ClassItem::Char('-')],
                    }),
                    ']' => State::Class(ClassAccumulator {
                        negated: false,
                        items: vec![ClassItem::Char(']')],
                    }),
                    '\\' => State::ClassEscape(ClassAccumulator {
                        negated: false,
                        items: Vec::new(),
                    }),
                    other => State::Class(ClassAccumulator {
                        negated: false,
                        items: vec![ClassItem::Char(other)],
                    }),
                };
                Ok(None)
            }
            None => Err(Error::UnclosedClass),
        }
    }

    /// Handle a character in a character class specification.
    fn handle_class(&mut self, mut acc: ClassAccumulator) -> StringResult {
        match self.pattern.next() {
            Some(chr) => Ok(match chr {
                ']' => {
                    if acc.items.is_empty() {
                        acc.items.push(ClassItem::Char(']'));
                        self.state = State::Class(acc);
                        None
                    } else {
                        self.state = State::Literal;
                        Some(close_class(acc))
                    }
                }
                '-' => match acc.items.pop() {
                    None => {
                        acc.items.push(ClassItem::Char('-'));
                        self.state = State::Class(acc);
                        None
                    }
                    Some(ClassItem::Range(start, end)) => {
                        acc.items.push(ClassItem::Range(start, end));
                        self.state = State::ClassRangeDash(acc);
                        None
                    }
                    Some(ClassItem::Char(start)) => {
                        self.state = State::ClassRange(acc, start);
                        None
                    }
                },
                '\\' => {
                    self.state = State::ClassEscape(acc);
                    None
                }
                other => {
                    acc.items.push(ClassItem::Char(other));
                    self.state = State::Class(acc);
                    None
                }
            }),
            None => Err(Error::UnclosedClass),
        }
    }

    /// Escape a character in a class specification.
    fn handle_class_escape(&mut self, mut acc: ClassAccumulator) -> StringResult {
        match self.pattern.next() {
            Some(chr) => {
                acc.items.push(ClassItem::Char(map_letter_escape(chr)));
                self.state = State::Class(acc);
                Ok(None)
            }
            None => Err(Error::UnclosedClass),
        }
    }

    /// Handle a character within a class range.
    fn handle_class_range(&mut self, mut acc: ClassAccumulator, start: char) -> StringResult {
        match self.pattern.next() {
            Some(chr) => match chr {
                '\\' => Err(Error::NotImplemented(format!(
                    "FIXME: handle class range end escape with {acc:?} start {start:?}"
                ))),
                ']' => {
                    acc.items.push(ClassItem::Char(start));
                    acc.items.push(ClassItem::Char('-'));
                    self.state = State::Literal;
                    Ok(Some(close_class(acc)))
                }
                end if start > end => Err(Error::ReversedRange(start, end)),
                end if start == end => {
                    acc.items.push(ClassItem::Char(start));
                    self.state = State::Class(acc);
                    Ok(None)
                }
                end => {
                    acc.items.push(ClassItem::Range(start, end));
                    self.state = State::Class(acc);
                    Ok(None)
                }
            },
            None => Err(Error::UnclosedClass),
        }
    }

    /// Handle a dash immediately following a range within a character class.
    #[allow(clippy::panic_in_result_fn)]
    #[allow(clippy::unreachable)]
    fn handle_class_range_dash(&mut self, mut acc: ClassAccumulator) -> StringResult {
        match self.pattern.next() {
            Some(chr) => {
                if chr == ']' {
                    acc.items.push(ClassItem::Char('-'));
                    self.state = State::Literal;
                    Ok(Some(close_class(acc)))
                } else if let Some(ClassItem::Range(start, end)) = acc.items.pop() {
                    Err(Error::RangeAfterRange(start, end))
                } else {
                    // Let's hope the optimiser hears us...
                    unreachable!()
                }
            }
            None => Err(Error::UnclosedClass),
        }
    }

    /// Start a set of alternatives.
    fn handle_alternate(&mut self, mut current: String, mut gathered: Vec<String>) -> StringResult {
        match self.pattern.next() {
            Some(chr) => match chr {
                ',' => {
                    gathered.push(current);
                    self.state = State::Alternate(String::new(), gathered);
                    Ok(None)
                }
                '}' => {
                    self.state = State::Literal;
                    if current.is_empty() && gathered.is_empty() {
                        Ok(Some(r"\{\}".to_owned()))
                    } else {
                        gathered.push(current);
                        Ok(Some(close_alternate(gathered)))
                    }
                }
                '\\' => {
                    self.state = State::AlternateEscape(current, gathered);
                    Ok(None)
                }
                '[' => Err(Error::NotImplemented(
                    "FIXME: alternate character class".to_owned(),
                )),
                other => {
                    current.push(other);
                    self.state = State::Alternate(current, gathered);
                    Ok(None)
                }
            },
            None => Err(Error::UnclosedAlternation),
        }
    }

    /// Escape a character within a list of alternatives.
    fn handle_alternate_escape(
        &mut self,
        mut current: String,
        gathered: Vec<String>,
    ) -> StringResult {
        match self.pattern.next() {
            Some(chr) => {
                current.push(map_letter_escape(chr));
                self.state = State::Alternate(current, gathered);
                Ok(None)
            }
            None => Err(Error::UnclosedAlternation),
        }
    }
}

impl<I> Iterator for GlobIterator<I>
where
    I: Iterator<Item = char>,
{
    type Item = StringResult;

    fn next(&mut self) -> Option<Self::Item> {
        match mem::take(&mut self.state) {
            State::Start => Some(Ok(Some(self.handle_start()))),
            State::End => None,
            State::Literal => Some(Ok(self.handle_literal())),
            State::Escape => Some(self.handle_escape()),
            State::ClassStart => Some(self.handle_class_start()),
            State::Class(acc) => Some(self.handle_class(acc)),
            State::ClassEscape(acc) => Some(self.handle_class_escape(acc)),
            State::ClassRange(acc, start) => Some(self.handle_class_range(acc, start)),
            State::ClassRangeDash(acc) => Some(self.handle_class_range_dash(acc)),
            State::Alternate(current, gathered) => Some(self.handle_alternate(current, gathered)),
            State::AlternateEscape(current, gathered) => {
                Some(self.handle_alternate_escape(current, gathered))
            }
        }
    }
}

// custom flatten_ok implementation to replace itertools functionality
fn flatten_ok<I, T, E>(iter: I) -> impl Iterator<Item = Result<T, E>>
where
    I: Iterator<Item = Result<Option<T>, E>>,
{
    iter.filter_map(|res| match res {
        Ok(Some(item)) => Some(Ok(item)),
        Ok(None) => None,
        Err(err) => Some(Err(err)),
    })
}

/// Parse a shell glob-like pattern into a regular expression.
///
/// See the module-level documentation for a description of the pattern
/// features supported.
///
/// # Errors
/// Most of the [`Error`] values, mostly syntax errors in
/// the specified glob pattern.
#[allow(clippy::missing_inline_in_public_items)]
pub fn glob_to_regex(pattern: &str) -> Result<Regex, Error> {
    let parser = GlobIterator {
        pattern: pattern.chars(),
        state: State::Start,
    };

    let mut result = Vec::new();
    for item in flatten_ok(parser) {
        result.push(item?);
    }

    let re_pattern = result.join("");
    Regex::new(&re_pattern).map_err(|err| Error::InvalidRegex(re_pattern, err.to_string()))
}
