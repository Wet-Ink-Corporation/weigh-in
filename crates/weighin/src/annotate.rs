//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A code line is only ever changed inside its last comment: the note goes at the end of a line
//! comment, or just before the closer of a block comment that ends the line. Only the markers of
//! the file's language ([`Lang`], from its name) open a comment, so Python's `a // b` and a shell
//! or YAML URL stay code; a file of unknown language reads them all. Comment markers inside a
//! string literal (a Rust raw string too) do not count; `//` glued to a `:` (`http://`) and `/*`
//! glued to a word (`build/**/*.o`) do not open a comment; `#`, `--` and `REM ` open one only
//! standing alone (at the line start or after whitespace, and `#` and `--` not before a word
//! character), so `r#"`, `x--` and `#define` stay code; Visual Basic's `'` opens one only
//! standing alone (before whitespace too) and not closing on the line, so a lifetime
//! (`+ 'static`) stays code; and a line-start `*` is a block comment's middle only before
//! whitespace and right under a line that leaves a block comment open, so `*p = 0;` and a
//! product continued from the line above stay code. A line opener can still be code (`#  define`,
//! `i --` in a file of unknown language), so the rest of the line is also read as code. The note
//! goes before the closer of a block comment that ends the line, which both readings keep in a
//! comment; or at the end of the line when the code reading finds no comment there either, and
//! the line comment is certain: the file's language has its marker, or it opens the line. Anything
//! else is left as it was, so a decision does not break the code around its question. The
//! reading is a line-local heuristic, not a parser: a line inside a string or comment opened on an
//! earlier line is read as if it started in code; a line comment glued to its marker (`#TODO`) is
//! left alone; and so is a line comment whose text, read as code, holds a comment marker inside
//! quotes, leaves a string open or closes a block comment before more text, one that ends in a
//! backslash, which splices the next line into it, and, in a file of unknown language, one whose
//! marker stands after code with nothing after it to confirm it.

mod lang;

pub(crate) use lang::Lang;
use lang::Marker;

/// How the source file reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    /// Prose (markdown, plain text and the like): the note goes at the end of the line.
    Prose,
    /// Anything else: the note goes only inside the line's comment.
    Code,
}

/// Why a source line was left as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The note holds a line break (a control character, U+2028 or U+2029) or a comment closer,
    /// or text that would change the block comment it goes in (`/*`, or `--` in `<!-- -->`).
    UnsafeText,
    /// Code follows the comment's closer on the line.
    CodeAfterComment,
    /// The line holds no comment the note could go in.
    NoComment,
    /// The line's only comment markers sit inside a string literal.
    InString,
    /// The line ends in a backslash, which splices the next line into its line comment.
    Spliced,
    /// The comment marker may be code: it stands after code in a file of unknown language, or it
    /// is a line-start `*` with no block comment open above it.
    Ambiguous,
}

impl Skip {
    /// The reason, as a receipt shows it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsafeText => "the option does not fit on one comment line",
            Self::CodeAfterComment => "code follows the comment",
            Self::NoComment => "no comment on the line",
            Self::InString => "the comment marker is inside a string",
            Self::Spliced => "the comment runs on to the next line",
            Self::Ambiguous => "the comment marker may be code",
        }
    }
}

/// The kind of comment an opener starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comment {
    /// Runs to the end of the line, whatever it holds.
    Line,
    /// `/* … */`, or a line in its middle that starts with `*`. Rust, Swift, Kotlin and Scala
    /// nest these, so a `/*` inside one opens another.
    Star,
    /// `<!-- … -->`, inside which XML allows no `--`.
    Html,
}

impl Comment {
    /// The closer of a block comment, `None` for a line comment.
    fn closer(self) -> Option<&'static str> {
        match self {
            Self::Line => None,
            Self::Star => Some("*/"),
            Self::Html => Some("-->"),
        }
    }

    /// Text that a note written inside this comment must not hold, besides the closer.
    fn forbidden(self) -> Option<&'static str> {
        match self {
            Self::Line => None,
            Self::Star => Some("/*"),
            Self::Html => Some("--"),
        }
    }
}

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

/// Block comment closers: text after one is code again.
const CLOSERS: &[&str] = &["*/", "-->"];

/// Quotes that open a string literal. A `'` opens one only when it closes on the line.
const QUOTES: &[char] = &['"', '`', '\''];

/// A comment found on a line.
#[derive(Debug, Clone, Copy)]
struct Found {
    /// Where its opener starts.
    start: usize,
    /// Where its body starts, just after the opener.
    body: usize,
    /// What kind of comment it is.
    kind: Comment,
}

/// What a stretch of code holds, up to the next comment or the end of the line.
struct CodeStretch {
    /// The next comment, if one opens before the line ends.
    comment: Option<Found>,
    /// Whether a string literal in the stretch holds a comment opener.
    marker_in_string: bool,
    /// Whether the stretch ends inside a `"` or backtick string that runs past the line.
    open_string: bool,
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
fn opener_at(line: &str, at: usize, lang: Lang) -> Option<Found> {
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
fn scan_code(line: &str, from: usize, lang: Lang) -> CodeStretch {
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

/// Whether `c` could end a line or a line comment: a control character, or the Unicode line
/// and paragraph separators, which end a `//` comment in JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Whether a comment opener of `lang` starts anywhere in `line` from byte `from` on.
fn opener_follows(line: &str, from: usize, lang: Lang) -> bool {
    line.get(from..).is_some_and(|tail| {
        tail.char_indices()
            .any(|(i, _)| opener_at(line, from.saturating_add(i), lang).is_some())
    })
}

/// Whether the opener at byte `start` of `line` opens the line, after indentation.
fn opens_line(line: &str, start: usize) -> bool {
    line.get(..start)
        .is_some_and(|before| before.trim().is_empty())
}

/// Whether `prev`, the line above, leaves a block comment open: it opens one after its last
/// closer, or is itself a middle line (`*`) that does not close it. `None` is the file's start.
fn leaves_block_open(prev: Option<&str>) -> bool {
    let Some(prev) = prev.map(str::trim) else {
        return false;
    };
    match (prev.rfind("/*"), prev.rfind("*/")) {
        (Some(open), Some(close)) => open > close,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => prev.starts_with('*'),
    }
}

/// How a file reads when a decision is written back into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Prose or code.
    pub(crate) syntax: Syntax,
    /// The comment syntax a code file is read in.
    pub(crate) lang: Lang,
}

/// `line` of a file read as `reading`, `prev` being the line above it (`None` for the first),
/// with `note` written into its comment, or why it cannot be.
///
/// # Errors
/// As [`annotate_line`], and [`Skip::Ambiguous`] for a block comment's middle line (`*`) with no
/// block comment open above it, which may be a product continued from that line.
pub(crate) fn annotate_source_line(
    prev: Option<&str>,
    line: &str,
    note: &str,
    reading: Reading,
) -> Result<String, Skip> {
    let Reading { syntax, lang } = reading;
    let written = annotate_line(line, note, syntax, lang)?;
    let middle_star = syntax == Syntax::Code
        && opener_at(
            line,
            line.len().saturating_sub(line.trim_start().len()),
            lang,
        )
        .is_some_and(|o| o.kind == Comment::Star && line.trim_start().starts_with('*'));
    if middle_star && !leaves_block_open(prev) {
        return Err(Skip::Ambiguous);
    }
    Ok(written)
}

/// `line`, read on its own as a line of a file in `lang`, with `note` written into its comment,
/// or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end or change the comment, or end the line;
/// [`Skip::NoComment`] for a code line with no comment; [`Skip::InString`] when its only comment
/// markers, or those in a line comment's text read as code, sit inside a string;
/// [`Skip::CodeAfterComment`] when its last comment, or one in a line comment's text read as
/// code, closes before the end of the line; [`Skip::Spliced`] when a line comment ends in a
/// backslash; [`Skip::Ambiguous`] when, in a file of unknown language, the comment the note
/// would go in may be code.
fn annotate_line(line: &str, note: &str, syntax: Syntax, lang: Lang) -> Result<String, Skip> {
    if note.chars().any(ends_a_line) || CLOSERS.iter().any(|c| note.contains(c)) {
        return Err(Skip::UnsafeText);
    }
    match syntax {
        Syntax::Prose => Ok(format!("{line} {note}")),
        Syntax::Code => annotate_code(line, note, lang),
    }
}

/// [`annotate_line`] for a code line: walks its comments in order and writes into the last one,
/// if the line ends inside it or with its closer.
///
/// A line opener may itself be code (`#  define`, `i --`, and in a file of unknown language
/// Python's `a // b`), so the rest of the line is read twice: as the comment's body and as code.
/// The note goes before the closer of a block comment that ends the line, which both readings
/// keep in a comment. It goes at the end of the line when the code reading ends in a comment that
/// runs on or finds none, and that comment is certain: `lang` is known, or a comment the line
/// ends in opens the line. When the code reading closes a comment before more code, or finds a
/// marker inside a string, the line is left alone.
fn annotate_code(line: &str, note: &str, lang: Lang) -> Result<String, Skip> {
    let mut from = 0;
    let mut marker_in_string = false;
    let mut closed_one = false;
    let mut line_opened = false;
    // Whether a comment the line ends in under one reading opens the line, so no reading of a
    // file of unknown language takes it for code.
    let mut certain = lang.known();
    loop {
        let stretch = scan_code(line, from, lang);
        marker_in_string |= stretch.marker_in_string;
        let Some(found) = stretch.comment else {
            return if closed_one {
                Err(Skip::CodeAfterComment)
            } else if marker_in_string || (line_opened && stretch.open_string) {
                Err(Skip::InString)
            } else if line_opened && line.trim_end().ends_with('\\') {
                // A backslash splices the next line into the comment; the note would end that.
                Err(Skip::Spliced)
            } else if line_opened && certain {
                // Read as code, the rest holds no comment: only the line comment reading stands.
                Ok(format!("{line} {note}"))
            } else if line_opened {
                Err(Skip::Ambiguous)
            } else {
                Err(Skip::NoComment)
            };
        };
        let Some(closer) = found.kind.closer() else {
            // Read the rest as code too, afresh: what came before the opener is settled.
            certain |= opens_line(line, found.start);
            line_opened = true;
            closed_one = false;
            marker_in_string = false;
            from = found.body;
            continue;
        };
        let body = line.get(found.body..).ok_or(Skip::NoComment)?;
        let written = match body.find(closer) {
            // A block comment that runs on: the line ends inside it, if it is one.
            None if certain || opens_line(line, found.start) => format!("{line} {note}"),
            None => return Err(Skip::Ambiguous),
            Some(at) => {
                let (before, closing) = line
                    .split_at_checked(found.body.saturating_add(at))
                    .ok_or(Skip::NoComment)?;
                let after = closing.get(closer.len()..).ok_or(Skip::NoComment)?;
                if !after.trim().is_empty() {
                    // Code follows the closer: the note can only go in a comment after it.
                    closed_one = true;
                    from = line.len().saturating_sub(after.len());
                    continue;
                }
                format!("{} {note} {closing}", before.trim_end())
            }
        };
        return match found.kind.forbidden() {
            Some(text) if note.contains(text) => Err(Skip::UnsafeText),
            _ => Ok(written),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "→ Weigh-In wi-abc123: decided 4096 (2026-10-07)";

    /// A C source file.
    const C: Reading = Reading {
        syntax: Syntax::Code,
        lang: Lang::CFamily,
    };

    #[test]
    fn a_line_comment_gets_the_note_at_the_end() {
        let out = annotate_line("    // TODO: pick a cap", NOTE, Syntax::Code, Lang::Unknown);

        assert_eq!(out, Ok(format!("    // TODO: pick a cap {NOTE}")));
    }

    #[test]
    fn a_block_comment_alone_gets_the_note_before_its_closer() {
        let out = annotate_line(
            "    /* TODO: pick a timeout */",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Ok(format!("    /* TODO: pick a timeout {NOTE} */")));
    }

    #[test]
    fn an_html_comment_gets_the_note_before_its_closer() {
        let out = annotate_line(
            "<!-- TODO: choose a layout -->  ",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Ok(format!("<!-- TODO: choose a layout {NOTE} -->  ")));
    }

    #[test]
    fn a_block_comment_followed_by_code_is_left_alone() {
        let out = annotate_line(
            "    /* TODO: pick a buffer size */ return 0;",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn a_code_line_without_a_comment_is_left_alone() {
        let out = annotate_line("let size = 4096;", NOTE, Syntax::Code, Lang::Unknown);

        assert_eq!(out, Err(Skip::NoComment));
    }

    #[test]
    fn a_comment_marker_inside_a_string_is_left_alone() {
        for line in [
            r#"const s = "// TODO: decide later what this is"; console.log(s);"#,
            r#"s = "a \" // TODO: escaped quote"; f(s)"#,
            "x = '# TODO: pick a colour'; f(x)",
            "const t = `/* TODO: pick a template */`; g(t);",
            r#"const u = "// TODO: runs past the line"#,
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::InString), "{line}");
        }
    }

    #[test]
    fn a_line_comment_after_a_string_holding_a_marker_gets_the_note_at_the_end() {
        let line = r#"let s = "a // b"; // TODO: pick a separator"#;

        let out = annotate_line(line, NOTE, Syntax::Code, Lang::Rust);

        assert_eq!(out, Ok(format!("{line} {NOTE}")));
    }

    #[test]
    fn a_line_comment_after_a_closed_block_comment_gets_the_note_at_the_end() {
        let line = "/* a */ int x; // TODO: pick a width";

        let out = annotate_line(line, NOTE, Syntax::Code, Lang::CFamily);

        assert_eq!(out, Ok(format!("{line} {NOTE}")));
    }

    #[test]
    fn the_last_of_two_block_comments_gets_the_note_before_its_closer() {
        let out = annotate_line(
            "/* a */ int x; /* TODO: pick a width */",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(
            out,
            Ok(format!("/* a */ int x; /* TODO: pick a width {NOTE} */"))
        );
    }

    #[test]
    fn a_quote_that_does_not_close_on_the_line_is_read_in_context() {
        for (line, lang) in [
            ("x = 1 ' TODO: pick a start", Lang::Basic),
            ("fn f(x: &'a str) {} // TODO: pick a name", Lang::Rust),
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, lang);

            assert_eq!(out, Ok(format!("{line} {NOTE}")), "{line}");
        }
    }

    #[test]
    fn a_block_comment_that_runs_past_the_line_gets_the_note_at_the_end() {
        let out = annotate_line(
            "/* TODO: pick a cap, because",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Ok(format!("/* TODO: pick a cap, because {NOTE}")));
    }

    #[test]
    fn a_markdown_line_gets_the_note_at_the_end() {
        let out = annotate_line(
            "- Should we shard by tenant?",
            NOTE,
            Syntax::Prose,
            Lang::Unknown,
        );

        assert_eq!(out, Ok(format!("- Should we shard by tenant? {NOTE}")));
    }

    #[test]
    fn a_note_that_would_break_the_line_is_refused() {
        for note in [
            "→ Weigh-In wi-abc123: decided 10\nreturn 99; (2026-10-07)",
            "→ Weigh-In wi-abc123: decided a\tb (2026-10-07)",
            "→ Weigh-In wi-abc123: decided 1 */ int z (2026-10-07)",
            "→ Weigh-In wi-abc123: decided --> <b>x</b> (2026-10-07)",
            "→ Weigh-In wi-abc123: decided a\u{2028}alert(1); (2026-10-07)",
            "→ Weigh-In wi-abc123: decided a\u{2029}alert(1); (2026-10-07)",
        ] {
            for syntax in [Syntax::Code, Syntax::Prose] {
                let out = annotate_line("// TODO: pick a cap", note, syntax, Lang::Unknown);

                assert_eq!(out, Err(Skip::UnsafeText), "{note:?}");
            }
        }
    }

    #[test]
    fn a_closer_inside_a_line_comment_does_not_end_it() {
        for line in [
            "    # TODO: exclude build/**/*.o",
            "// TODO: is a*/b right?",
            "-- TODO: map a --> b",
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Ok(format!("{line} {NOTE}")), "{line}");
        }
    }

    #[test]
    fn a_hash_inside_code_does_not_open_a_comment() {
        let out = annotate_line(
            r##"    let s = r#"// TODO: x"#; f(s);"##,
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Err(Skip::InString));
    }

    #[test]
    fn a_block_comment_after_dashes_or_a_directive_gets_the_note_before_its_closer() {
        for (line, want) in [
            (
                "x--; /* TODO: pick */",
                format!("x--; /* TODO: pick {NOTE} */"),
            ),
            (
                "    --count; /* TODO: pick */",
                format!("    --count; /* TODO: pick {NOTE} */"),
            ),
            (
                "#define CAP 1 /* TODO: pick */",
                format!("#define CAP 1 /* TODO: pick {NOTE} */"),
            ),
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Ok(want), "{line}");
        }
    }

    #[test]
    fn a_note_that_would_open_a_nested_comment_is_refused_inside_a_block_comment() {
        let star = "→ Weigh-In wi-abc123: decided a /* b (2026-10-07)";
        let dashes = "→ Weigh-In wi-abc123: decided --force (2026-10-07)";
        for (line, note) in [
            ("/* TODO: pick */", star),
            ("/* TODO: pick, because", star),
            (" * TODO: pick", star),
            ("<!-- TODO: pick -->", dashes),
        ] {
            let out = annotate_line(line, note, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::UnsafeText), "{line}");
        }
    }

    #[test]
    fn a_note_with_an_opener_still_fits_a_line_comment() {
        let note = "→ Weigh-In wi-abc123: decided a /* b --force (2026-10-07)";

        let out = annotate_line("// TODO: pick", note, Syntax::Code, Lang::Unknown);

        assert_eq!(out, Ok(format!("// TODO: pick {note}")));
    }

    #[test]
    fn a_lifetime_before_a_block_comment_does_not_open_a_comment() {
        let ok = annotate_line(
            "fn f<T: 'static>() {} /* TODO: x */",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );
        let skipped = annotate_line(
            "impl<T: 'static> X for T { /* TODO: x */ }",
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(
            ok,
            Ok(format!("fn f<T: 'static>() {{}} /* TODO: x {NOTE} */"))
        );
        assert_eq!(skipped, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn code_after_a_comment_is_the_reason_even_when_a_later_string_holds_a_marker() {
        let out = annotate_line(
            r#"/* TODO: x */ s = "//";"#,
            NOTE,
            Syntax::Code,
            Lang::Unknown,
        );

        assert_eq!(out, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn a_double_slash_in_code_leaves_the_note_in_the_block_comment_that_ends_the_line() {
        for (line, want) in [
            (
                "a { background: url(http://x/a.png); } /* TODO: pick a size */",
                format!("a {{ background: url(http://x/a.png); }} /* TODO: pick a size {NOTE} */"),
            ),
            (
                "<p>See http://example.com</p> <!-- TODO: choose a layout -->",
                format!("<p>See http://example.com</p> <!-- TODO: choose a layout {NOTE} -->"),
            ),
            (
                "<img src=//cdn/x.png> <!-- TODO: x -->",
                format!("<img src=//cdn/x.png> <!-- TODO: x {NOTE} -->"),
            ),
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Ok(want), "{line}");
        }
    }

    #[test]
    fn a_hash_or_dashes_standing_as_code_leave_the_note_in_the_block_comment() {
        for (line, want) in [
            (
                "#  define CAP 1 /* TODO: pick a cap */",
                format!("#  define CAP 1 /* TODO: pick a cap {NOTE} */"),
            ),
            (
                "# if A /* TODO: x */",
                format!("# if A /* TODO: x {NOTE} */"),
            ),
            (
                "    i --; /* TODO: pick a count */",
                format!("    i --; /* TODO: pick a count {NOTE} */"),
            ),
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Ok(want), "{line}");
        }
    }

    #[test]
    fn a_line_opener_that_may_be_code_is_left_alone_when_the_rest_would_break() {
        for (line, want) in [
            (
                r##"x = a // b; s = "# TODO: decide this x""##,
                Skip::InString,
            ),
            (
                ".a { background: url(http://x/a.png); /* TODO: size */ }",
                Skip::CodeAfterComment,
            ),
            ("#  define CAP /* TODO: pick */ 1", Skip::CodeAfterComment),
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(want), "{line}");
        }
    }

    #[test]
    fn a_lifetime_after_whitespace_does_not_open_a_comment() {
        for line in [
            r#"fn f(x: impl Fn() + 'static) { panic!("TODO: pick a limit") }"#,
            r#"where T: 'a + Send { todo!("FIXME later") }"#,
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::NoComment), "{line}");
        }
    }

    #[test]
    fn a_code_line_starting_with_a_star_is_not_a_block_comment() {
        for line in [
            r#"    *limit = todo!("pick a cap");"#,
            "    *p = 0; panic!(\"TODO: x\")",
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::NoComment), "{line}");
        }
    }

    #[test]
    fn a_block_comments_middle_star_still_gets_the_note() {
        for line in [" * TODO: pick a cap", "\t*\tTODO: pick a cap"] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Ok(format!("{line} {NOTE}")), "{line}");
        }
    }

    #[test]
    fn a_line_comment_whose_text_read_as_code_leaves_a_string_open_is_left_alone() {
        let line = r#"fn f(s: &'static str) -> bool { s == "'" || s == "// TODO: decide" }"#;

        let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

        assert_eq!(out, Err(Skip::InString));
    }

    #[test]
    fn a_line_comment_ending_in_a_line_splice_is_left_alone() {
        for line in [
            r"#define X 1 // TODO: q9 word9 \",
            "    // TODO: pick a cap \\  ",
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::Spliced), "{line}");
        }
    }

    #[test]
    fn a_quote_and_space_that_closes_on_the_line_is_read_as_a_string() {
        for line in ["x = 1 ' TODO: it's fine", "s = ' TODO: pick it'"] {
            let out = annotate_line(line, NOTE, Syntax::Code, Lang::Unknown);

            assert_eq!(out, Err(Skip::InString), "{line}");
        }
    }

    /// Where the note should go in a corpus line: at the end, before the last block closer, or
    /// nowhere (left alone, for the given reason).
    #[derive(Debug, Clone, Copy)]
    enum Want {
        End,
        BeforeCloser(&'static str),
        Skipped(Skip),
    }

    /// Real code lines per language, with where the note should go in each.
    const CORPUS: &[(&str, &str, Want)] = &[
        ("rust", "    // TODO: pick a cap", Want::End),
        ("rust", "fn f<'a>(x: &'a str) {} // TODO: pick", Want::End),
        (
            "rust",
            "fn f(x: impl Fn() + 'static) { todo!(\"TODO: x\") }",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "rust",
            "where T: 'a + Send { todo!(\"FIXME later\") }",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "rust",
            "    *limit = todo!(\"pick a cap\");",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "rust",
            "let c = ' '; todo!(\"TODO: x\")",
            Want::Skipped(Skip::NoComment),
        ),
        ("rust", "let c = ' '; // TODO: pick a separator", Want::End),
        (
            "rust",
            "    let s = r#\"// TODO: x\"#; f(s);",
            Want::Skipped(Skip::InString),
        ),
        (
            "c",
            "#define X 1 // TODO: q9 word9 \\",
            Want::Skipped(Skip::Spliced),
        ),
        (
            "c",
            "    x = a * b; /* TODO: pick a scale */",
            Want::BeforeCloser("*/"),
        ),
        ("c", " * TODO: pick a width", Want::End),
        ("c", "    *p = 0; // TODO: pick a default", Want::End),
        ("js", "const u = `// ${a}`; // TODO: pick", Want::End),
        (
            "js",
            "const s = \"// TODO: x\"; f(s);",
            Want::Skipped(Skip::InString),
        ),
        ("python", "x = a // b  # TODO: pick a divisor", Want::End),
        (
            "python",
            "s = ' TODO: pick it'",
            Want::Skipped(Skip::InString),
        ),
        ("python", "s = '#'  # TODO: pick a marker", Want::End),
        ("sql", "SELECT '--' AS d; -- TODO: pick a dash", Want::End),
        (
            "sql",
            "SELECT 1; /* TODO: pick a limit */",
            Want::BeforeCloser("*/"),
        ),
        (
            "html",
            "<p>x</p> <!-- TODO: choose a layout -->",
            Want::BeforeCloser("-->"),
        ),
        ("shell", "echo \"# not\" # TODO: pick a flag", Want::End),
        ("shell", "    i=$((i--)) # TODO: pick a step", Want::End),
        ("vb", "x = 1 ' TODO: pick a start", Want::End),
        (
            "vb",
            "x = 1 ' TODO: it's fine",
            Want::Skipped(Skip::InString),
        ),
        ("batch", "REM TODO: pick a drive", Want::End),
        (
            "python",
            "half = len(items) // 2 if use_temporary_cache else 0",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "shell",
            "curl -s https://jsonplaceholder.typicode.com/todo/items | jq .",
            Want::Skipped(Skip::NoComment),
        ),
        ("shell", "echo ' TODO: pick", Want::Skipped(Skip::NoComment)),
        (
            "dockerfile",
            "RUN curl -fsSL https://example.com/temporary-install.sh | sh",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "yaml",
            "- run: wget http://host/todo/list.txt",
            Want::Skipped(Skip::NoComment),
        ),
        (
            "yaml",
            "url: http://example.com/temporary",
            Want::Skipped(Skip::NoComment),
        ),
        ("yaml", "retries: 3  # TODO: pick a count", Want::End),
        ("c", "x = y -- TODO_count;", Want::Skipped(Skip::NoComment)),
        (
            "rust",
            "let s = r#\"a\"b // TODO \"x\"#;",
            Want::Skipped(Skip::InString),
        ),
        ("rust", "let s = r\"a\\\"; // TODO: pick a path", Want::End),
        (
            "unknown",
            "half = n // 2 if temporary else 0",
            Want::Skipped(Skip::Ambiguous),
        ),
        (
            "unknown",
            "x = 1 ' TODO: pick a start",
            Want::Skipped(Skip::Ambiguous),
        ),
        (
            "unknown",
            "ls /* # TODO: pick a glob",
            Want::Skipped(Skip::Ambiguous),
        ),
        (
            "unknown",
            "curl http://x/todo | sh",
            Want::Skipped(Skip::NoComment),
        ),
        ("unknown", "    // TODO: see http://x", Want::End),
        ("unknown", "# TODO: a // b", Want::End),
    ];

    /// The language a corpus row names.
    fn corpus_lang(name: &str) -> Lang {
        match name {
            "rust" => Lang::Rust,
            "c" | "js" => Lang::CFamily,
            "python" | "shell" | "yaml" | "dockerfile" => Lang::Hash,
            "sql" => Lang::Sql,
            "html" => Lang::Markup,
            "vb" => Lang::Basic,
            "batch" => Lang::Batch,
            _ => Lang::Unknown,
        }
    }

    #[test]
    fn a_corpus_of_real_lines_is_skipped_or_gets_the_note_inside_its_comment() {
        for &(lang, line, want) in CORPUS {
            let out = annotate_line(line, NOTE, Syntax::Code, corpus_lang(lang));

            match want {
                Want::End => assert_eq!(out, Ok(format!("{line} {NOTE}")), "{lang}: {line}"),
                Want::BeforeCloser(closer) => {
                    let (head, _) = line.rsplit_once(closer).expect("the line has the closer");
                    let expected = format!("{} {NOTE} {closer}", head.trim_end());
                    assert_eq!(out, Ok(expected), "{lang}: {line}");
                }
                Want::Skipped(skip) => assert_eq!(out, Err(skip), "{lang}: {line}"),
            }
        }
    }

    #[test]
    fn a_middle_star_gets_the_note_only_under_an_open_block_comment() {
        let line = " * TODO: pick a cap";
        for (prev, open) in [
            (Some("/**"), true),
            (Some(" * Caps the queue."), true),
            (Some("int x; /* see below"), true),
            (None, false),
            (Some("x = a"), false),
            (Some("/* a */"), false),
            (Some(" * done */"), false),
        ] {
            let out = annotate_source_line(prev, line, NOTE, C);

            let want = if open {
                Ok(format!("{line} {NOTE}"))
            } else {
                Err(Skip::Ambiguous)
            };
            assert_eq!(out, want, "{prev:?}");
        }
    }

    #[test]
    fn a_line_comment_under_any_line_gets_the_note() {
        let line = "    // TODO: pick a cap";

        let out = annotate_source_line(Some("x = a"), line, NOTE, C);

        assert_eq!(out, Ok(format!("{line} {NOTE}")));
    }

    #[test]
    fn rem_opens_a_comment_at_the_line_start() {
        let out = annotate_line("REM TODO: pick a drive", NOTE, Syntax::Code, Lang::Unknown);

        assert_eq!(out, Ok(format!("REM TODO: pick a drive {NOTE}")));
    }
}
