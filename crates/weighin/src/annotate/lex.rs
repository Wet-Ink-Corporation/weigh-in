//! The line-local tokeniser of the write-back: where the comment openers of a file's language
//! stand on a line read as code, skipping string literals. It is a heuristic, not a parser; the
//! guard ([`super::guard`]) refuses what it may have read wrongly.

use super::Comment;
use super::lang::{Lang, Marker};

/// Where an opener may stand on a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Anywhere outside a string literal.
    Anywhere,
    /// Anywhere outside a string literal, but not glued to a word character before it: so the
    /// `/*` of a path or glob (`build/**/*.o`) stays text.
    AfterNonWord,
    /// Anywhere outside a string literal, but not glued to a `:` before it: so the `//` of a URL
    /// (`http://`, `file://`) stays text.
    AfterNonColon,
    /// Only at the start of the line, after indentation, and before whitespace or the end of the
    /// line (a block comment's middle `*`): so a deref or a product (`*p = 0;`) stays code.
    LineStart,
    /// At the start of the line or after whitespace, and, unless the opener ends in a space
    /// (`REM `), not followed by a word character, `[` or `!`: so `r#"`, `x--`, `#define`,
    /// `#[derive]` and `--count` stay code.
    Standalone,
}

/// A comment opener: its text, the comment it opens, where it may stand and the marker a
/// language must have for it to count.
struct Opener {
    text: &'static str,
    kind: Comment,
    place: Place,
    marker: Marker,
}

/// The comment openers `weighin-core`'s TODO scan recognises, so every line it raised from is
/// read the same way here, longest first. The scan's `'` is read in context instead
/// ([`scan_code`]).
const OPENERS: &[Opener] = &[
    Opener {
        text: "<!--",
        kind: Comment::Html,
        place: Place::Anywhere,
        marker: Marker::Html,
    },
    Opener {
        text: "/*",
        kind: Comment::Star,
        place: Place::AfterNonWord,
        marker: Marker::Star,
    },
    Opener {
        text: "//",
        kind: Comment::Line,
        place: Place::AfterNonColon,
        marker: Marker::Slash,
    },
    Opener {
        text: "--",
        kind: Comment::Line,
        place: Place::Standalone,
        marker: Marker::Dashes,
    },
    Opener {
        text: "#",
        kind: Comment::Line,
        place: Place::Standalone,
        marker: Marker::Hash,
    },
    Opener {
        text: "REM ",
        kind: Comment::Line,
        place: Place::Standalone,
        marker: Marker::Rem,
    },
    Opener {
        text: "*",
        kind: Comment::Star,
        place: Place::LineStart,
        marker: Marker::Star,
    },
];

/// Quotes that open a string literal. A `'` opens one only when it closes on the line.
pub(super) const QUOTES: &[char] = &['"', '`', '\''];

/// A comment found on a line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Found {
    /// Where its opener starts.
    pub(super) start: usize,
    /// Where its body starts, just after the opener.
    pub(super) body: usize,
    /// What kind of comment it is.
    pub(super) kind: Comment,
}

/// What a stretch of code holds, up to the next comment or the end of the line.
pub(super) struct CodeStretch {
    /// The next comment, if one opens before the line ends.
    pub(super) comment: Option<Found>,
    /// Whether a string literal in the stretch holds a comment opener.
    pub(super) marker_in_string: bool,
    /// Whether the stretch ends inside a `"` or backtick string that runs past the line.
    pub(super) open_string: bool,
}

/// Whether `before` ends where a standalone opener may start: at the line start or whitespace.
fn after_space(before: &str) -> bool {
    before.chars().next_back().is_none_or(char::is_whitespace)
}

/// Whether `after`, the text after an opener, starts with whitespace or is empty.
fn space_or_end(after: Option<&str>) -> bool {
    after.is_some_and(|t| t.chars().next().is_none_or(char::is_whitespace))
}

/// Whether `c` belongs to a word: a letter, a digit or `_`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `opener`, which `tail` starts with and `before` precedes, stands where `place` allows.
fn stands(opener: &str, place: Place, before: &str, tail: &str) -> bool {
    match place {
        Place::Anywhere => true,
        Place::AfterNonWord => !before.chars().next_back().is_some_and(is_word),
        Place::AfterNonColon => !before.ends_with(':'),
        Place::LineStart => before.trim().is_empty() && space_or_end(tail.get(opener.len()..)),
        Place::Standalone => {
            let next = tail.get(opener.len()..).and_then(|t| t.chars().next());
            after_space(before)
                && (opener.ends_with(' ')
                    || !next.is_some_and(|c| is_word(c) || matches!(c, '[' | '!')))
        }
    }
}

/// The comment that an opener of `lang` at byte `at` of `line` starts, if one does.
pub(super) fn opener_at(line: &str, at: usize, lang: Lang) -> Option<Found> {
    let tail = line.get(at..)?;
    let before = line.get(..at)?;
    OPENERS
        .iter()
        .find(|o| {
            lang.allows(o.marker)
                && tail.starts_with(o.text)
                && stands(o.text, o.place, before, tail)
        })
        .map(|o| Found {
            start: at,
            body: at.saturating_add(o.text.len()),
            kind: o.kind,
        })
}

/// What starts at a byte of a line read as code.
enum Token {
    /// A comment.
    Comment(Found),
    /// A string literal that closes on the line: its byte length, quotes included, and whether
    /// it holds something the scan reads as a comment marker.
    Str { len: usize, marker: bool },
    /// A string literal that runs past the line.
    OpenStr,
    /// Any other character, of this byte length.
    Other(usize),
}

/// The raw string literal (`r"…"`, `r#"…"#`, `br"…"`) whose `r` is at byte `at` of `line`, if
/// one starts there. A backslash escapes nothing in it.
fn raw_string(line: &str, at: usize) -> Option<Token> {
    let before = line.get(..at)?;
    let prefix = before.strip_suffix(['b', 'c']).unwrap_or(before);
    if prefix.chars().next_back().is_some_and(is_word) {
        return None;
    }
    let tail = line.get(at..)?;
    let after_r = tail.strip_prefix('r')?;
    let after_hashes = after_r.trim_start_matches('#');
    let hashes = after_r.len().saturating_sub(after_hashes.len());
    let body = after_hashes.strip_prefix('"')?;
    let closer = format!("\"{}", "#".repeat(hashes));
    let opened = hashes.saturating_add(2);
    Some(body.find(&closer).map_or(Token::OpenStr, |i| {
        let len = opened.saturating_add(i).saturating_add(closer.len());
        Token::Str {
            len,
            marker: tail.get(..len).is_some_and(holds_opener),
        }
    }))
}

/// The token of `lang` code that starts at byte `at` of `line`; `None` at the end of the line.
/// Where `lang` has Visual Basic's `'`, a `'` that does not close on the line opens a comment
/// when it stands alone (after whitespace or at the line start, and before whitespace or the end
/// of the line) and no other opener follows it on the line; otherwise it is an ordinary
/// character (a Rust lifetime, an OCaml type variable, a shell string running on). A `'` string
/// that closes on the line but opens with whitespace (`' TODO: it's'`) is what the scan read as
/// that comment, so it counts as a string holding a marker. In Rust, a raw string is one token.
fn token_at(line: &str, at: usize, lang: Lang) -> Option<Token> {
    let tail = line.get(at..)?;
    let c = tail.chars().next()?;
    if let Some(found) = opener_at(line, at, lang) {
        return Some(Token::Comment(found));
    }
    if lang == Lang::Rust
        && c == 'r'
        && let Some(raw) = raw_string(line, at)
    {
        return Some(raw);
    }
    if !QUOTES.contains(&c) {
        return Some(Token::Other(c.len_utf8()));
    }
    Some(match string_len(tail, c) {
        Some(len) => {
            let text = tail.get(..len);
            Token::Str {
                len,
                marker: text.is_some_and(holds_opener)
                    || (c == '\'' && vb_opener(line, at) && text.is_some_and(not_a_char)),
            }
        }
        None if c != '\'' => Token::OpenStr,
        None if lang.allows(Marker::Quote)
            && vb_opener(line, at)
            && !opener_follows(line, at.saturating_add(1), lang) =>
        {
            Token::Comment(Found {
                start: at,
                body: at.saturating_add(1),
                kind: Comment::Line,
            })
        }
        None => Token::Other(c.len_utf8()),
    })
}

/// The byte length of the string literal that `quote`, the first character of `tail`, opens,
/// both quotes included; `None` when it does not close on the line. A backslash escapes the
/// character after it.
fn string_len(tail: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    tail.char_indices().skip(1).find_map(|(i, c)| {
        if escaped {
            escaped = false;
            None
        } else if c == '\\' {
            escaped = true;
            None
        } else {
            (c == quote).then(|| i.saturating_add(c.len_utf8()))
        }
    })
}

/// Whether `text`, a string literal, holds something the scan reads as a comment opener, in any
/// language: the scan raised from it.
fn holds_opener(text: &str) -> bool {
    OPENERS
        .iter()
        .any(|o| o.place != Place::LineStart && text.contains(o.text))
}

/// Reads `line` as code of `lang` from byte `from`, skipping string literals, up to the next
/// comment ([`token_at`]).
pub(super) fn scan_code(line: &str, from: usize, lang: Lang) -> CodeStretch {
    let mut marker_in_string = false;
    let mut at = from;
    while let Some(token) = token_at(line, at, lang) {
        let step = match token {
            Token::Comment(found) => {
                return CodeStretch {
                    comment: Some(found),
                    marker_in_string,
                    open_string: false,
                };
            }
            Token::OpenStr => {
                // The string runs past the line: nothing after its quote is code.
                return CodeStretch {
                    comment: None,
                    marker_in_string: marker_in_string || line.get(at..).is_some_and(holds_opener),
                    open_string: true,
                };
            }
            Token::Str { len, marker } => {
                marker_in_string |= marker;
                len
            }
            Token::Other(len) => len,
        };
        at = at.saturating_add(step);
    }
    CodeStretch {
        comment: None,
        marker_in_string,
        open_string: false,
    }
}

/// Whether `text`, a quoted literal, holds more than one character: so it is not a character
/// literal such as `' '`.
fn not_a_char(text: &str) -> bool {
    text.chars().count() > 3
}

/// Whether the `'` at byte `at` of `line` stands alone, as VB's comment opener does: after
/// whitespace or at the line start, and before whitespace or the end of the line.
fn vb_opener(line: &str, at: usize) -> bool {
    line.get(..at).is_some_and(after_space) && space_or_end(line.get(at.saturating_add(1)..))
}

/// Whether a comment opener of `lang` starts anywhere in `line` from byte `from` on.
fn opener_follows(line: &str, from: usize, lang: Lang) -> bool {
    line.get(from..).is_some_and(|tail| {
        tail.char_indices()
            .any(|(i, _)| opener_at(line, from.saturating_add(i), lang).is_some())
    })
}
