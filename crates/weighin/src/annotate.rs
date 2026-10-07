//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A code line is only ever changed inside its last comment: the note goes at the end of a line
//! comment, or just before the closer of a block comment that ends the line. Comment markers
//! inside a string literal do not count, and `#`, `--` and `REM ` open a comment only standing
//! alone (at the line start or after whitespace, not before a word character), so `r#"`, `x--`
//! and `#define` stay code. Anything else is left as it was, so a decision does not break the
//! code around its question. The reading is a line-local heuristic, not a parser: a line inside
//! a string or comment opened on an earlier line is read as if it started in code, and a line
//! comment glued to its marker (`#TODO`) is left alone.

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
    /// The note holds a line break, another control character or a comment closer, or text
    /// that would change the block comment it goes in (`/*`, or `--` in `<!-- -->`).
    UnsafeText,
    /// Code follows the comment's closer on the line.
    CodeAfterComment,
    /// The line holds no comment the note could go in.
    NoComment,
    /// The line's only comment markers sit inside a string literal.
    InString,
}

impl Skip {
    /// The reason, as a receipt shows it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsafeText => "the option does not fit on one comment line",
            Self::CodeAfterComment => "code follows the comment",
            Self::NoComment => "no comment on the line",
            Self::InString => "the comment marker is inside a string",
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
    /// Only at the start of the line, after indentation (a block comment's middle `*`).
    LineStart,
    /// At the start of the line or after whitespace, and not followed by a word character, `[`
    /// or `!`: so `r#"`, `x--`, `#define`, `#[derive]` and `--count` stay code.
    Standalone,
}

/// The comment openers `weighin-core`'s TODO scan recognises, so every line it raised from is
/// read the same way here, longest first, with the comment each opens and where it may stand.
/// The scan's `'` is read in context instead ([`scan_code`]).
const OPENERS: &[(&str, Comment, Place)] = &[
    ("<!--", Comment::Html, Place::Anywhere),
    ("/*", Comment::Star, Place::Anywhere),
    ("//", Comment::Line, Place::Anywhere),
    ("--", Comment::Line, Place::Standalone),
    ("#", Comment::Line, Place::Standalone),
    ("REM ", Comment::Line, Place::Standalone),
    ("*", Comment::Star, Place::LineStart),
];

/// Block comment closers: text after one is code again.
const CLOSERS: &[&str] = &["*/", "-->"];

/// Quotes that open a string literal. A `'` opens one only when it closes on the line.
const QUOTES: &[char] = &['"', '`', '\''];

/// What a stretch of code holds, up to the next comment or the end of the line.
struct CodeStretch {
    /// Where the next comment's body starts and what kind it is, if a comment opens before the
    /// line ends.
    comment: Option<(usize, Comment)>,
    /// Whether a string literal in the stretch holds a comment opener.
    marker_in_string: bool,
}

/// Whether `before` ends where a standalone opener may start: at the line start or whitespace.
fn after_space(before: &str) -> bool {
    before.chars().next_back().is_none_or(char::is_whitespace)
}

/// The comment opener at byte `at` of `line`, if one starts there: its length and kind.
fn opener_at(line: &str, at: usize) -> Option<(usize, Comment)> {
    let tail = line.get(at..)?;
    let before = line.get(..at)?;
    OPENERS
        .iter()
        .find(|(m, _, place)| {
            tail.starts_with(m)
                && match place {
                    Place::Anywhere => true,
                    Place::LineStart => before.trim().is_empty(),
                    Place::Standalone => {
                        after_space(before)
                            && !tail
                                .get(m.len()..)
                                .and_then(|t| t.chars().next())
                                .is_some_and(|c| {
                                    c.is_alphanumeric() || matches!(c, '_' | '[' | '!')
                                })
                    }
                }
        })
        .map(|(m, kind, _)| (m.len(), *kind))
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

/// Whether `text`, a string literal, holds something that would open a comment outside it.
fn holds_opener(text: &str) -> bool {
    OPENERS
        .iter()
        .any(|(m, _, place)| *place != Place::LineStart && text.contains(m))
}

/// Reads `line` as code from byte `from`, skipping string literals, up to the next comment.
/// A `'` that does not close on the line opens a comment (VB) when it follows whitespace or
/// starts the line, and is an ordinary character (a Rust lifetime) otherwise.
fn scan_code(line: &str, from: usize) -> CodeStretch {
    let mut marker_in_string = false;
    let mut at = from;
    while let Some(tail) = line.get(at..) {
        let Some(c) = tail.chars().next() else {
            break;
        };
        if let Some((len, kind)) = opener_at(line, at) {
            return CodeStretch {
                comment: Some((at.saturating_add(len), kind)),
                marker_in_string,
            };
        }
        let step = if QUOTES.contains(&c) {
            match string_len(tail, c) {
                Some(len) => {
                    marker_in_string |= tail.get(..len).is_some_and(holds_opener);
                    len
                }
                None if c != '\'' => {
                    // The string runs past the line: nothing after its quote is code.
                    marker_in_string |= holds_opener(tail);
                    break;
                }
                None if line.get(..at).is_some_and(after_space) => {
                    return CodeStretch {
                        comment: Some((at.saturating_add(1), Comment::Line)),
                        marker_in_string,
                    };
                }
                None => c.len_utf8(),
            }
        } else {
            c.len_utf8()
        };
        at = at.saturating_add(step);
    }
    CodeStretch {
        comment: None,
        marker_in_string,
    }
}

/// `line` with `note` written into its comment, or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end or change the comment, or end the line;
/// [`Skip::NoComment`] for a code line with no comment; [`Skip::InString`] when its only comment
/// markers sit inside a string; [`Skip::CodeAfterComment`] when its last comment closes before
/// the end of the line.
pub(crate) fn annotate_line(line: &str, note: &str, syntax: Syntax) -> Result<String, Skip> {
    if note.chars().any(char::is_control) || CLOSERS.iter().any(|c| note.contains(c)) {
        return Err(Skip::UnsafeText);
    }
    match syntax {
        Syntax::Prose => Ok(format!("{line} {note}")),
        Syntax::Code => annotate_code(line, note),
    }
}

/// [`annotate_line`] for a code line: walks its comments in order and writes into the last one,
/// if the line ends inside it or with its closer.
fn annotate_code(line: &str, note: &str) -> Result<String, Skip> {
    let mut from = 0;
    let mut marker_in_string = false;
    let mut closed_one = false;
    loop {
        let stretch = scan_code(line, from);
        marker_in_string |= stretch.marker_in_string;
        let Some((body_at, kind)) = stretch.comment else {
            return Err(if marker_in_string {
                Skip::InString
            } else if closed_one {
                Skip::CodeAfterComment
            } else {
                Skip::NoComment
            });
        };
        let Some(closer) = kind.closer() else {
            // A line comment runs to the end of the line, whatever its body holds.
            return Ok(format!("{line} {note}"));
        };
        let body = line.get(body_at..).ok_or(Skip::NoComment)?;
        let written = match body.find(closer) {
            // A block comment that runs on: the line ends inside it.
            None => format!("{line} {note}"),
            Some(at) => {
                let (before, closing) = line
                    .split_at_checked(body_at.saturating_add(at))
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
        return match kind.forbidden() {
            Some(text) if note.contains(text) => Err(Skip::UnsafeText),
            _ => Ok(written),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "→ Weigh-In wi-abc123: decided 4096 (2026-10-07)";

    #[test]
    fn a_line_comment_gets_the_note_at_the_end() {
        let out = annotate_line("    // TODO: pick a cap", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("    // TODO: pick a cap {NOTE}")));
    }

    #[test]
    fn a_block_comment_alone_gets_the_note_before_its_closer() {
        let out = annotate_line("    /* TODO: pick a timeout */", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("    /* TODO: pick a timeout {NOTE} */")));
    }

    #[test]
    fn an_html_comment_gets_the_note_before_its_closer() {
        let out = annotate_line("<!-- TODO: choose a layout -->  ", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("<!-- TODO: choose a layout {NOTE} -->  ")));
    }

    #[test]
    fn a_block_comment_followed_by_code_is_left_alone() {
        let out = annotate_line(
            "    /* TODO: pick a buffer size */ return 0;",
            NOTE,
            Syntax::Code,
        );

        assert_eq!(out, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn a_code_line_without_a_comment_is_left_alone() {
        let out = annotate_line("let size = 4096;", NOTE, Syntax::Code);

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
            let out = annotate_line(line, NOTE, Syntax::Code);

            assert_eq!(out, Err(Skip::InString), "{line}");
        }
    }

    #[test]
    fn a_line_comment_after_a_string_holding_a_marker_gets_the_note_at_the_end() {
        let line = r#"let s = "a // b"; // TODO: pick a separator"#;

        let out = annotate_line(line, NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("{line} {NOTE}")));
    }

    #[test]
    fn a_line_comment_after_a_closed_block_comment_gets_the_note_at_the_end() {
        let line = "/* a */ int x; // TODO: pick a width";

        let out = annotate_line(line, NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("{line} {NOTE}")));
    }

    #[test]
    fn the_last_of_two_block_comments_gets_the_note_before_its_closer() {
        let out = annotate_line(
            "/* a */ int x; /* TODO: pick a width */",
            NOTE,
            Syntax::Code,
        );

        assert_eq!(
            out,
            Ok(format!("/* a */ int x; /* TODO: pick a width {NOTE} */"))
        );
    }

    #[test]
    fn a_quote_that_does_not_close_on_the_line_is_read_in_context() {
        for line in [
            "x = 1 ' TODO: pick a start",
            "fn f(x: &'a str) {} // TODO: pick a name",
        ] {
            let out = annotate_line(line, NOTE, Syntax::Code);

            assert_eq!(out, Ok(format!("{line} {NOTE}")), "{line}");
        }
    }

    #[test]
    fn a_block_comment_that_runs_past_the_line_gets_the_note_at_the_end() {
        let out = annotate_line("/* TODO: pick a cap, because", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("/* TODO: pick a cap, because {NOTE}")));
    }

    #[test]
    fn a_markdown_line_gets_the_note_at_the_end() {
        let out = annotate_line("- Should we shard by tenant?", NOTE, Syntax::Prose);

        assert_eq!(out, Ok(format!("- Should we shard by tenant? {NOTE}")));
    }

    #[test]
    fn a_note_that_would_break_the_line_is_refused() {
        for note in [
            "→ Weigh-In wi-abc123: decided 10\nreturn 99; (2026-10-07)",
            "→ Weigh-In wi-abc123: decided a\tb (2026-10-07)",
            "→ Weigh-In wi-abc123: decided 1 */ int z (2026-10-07)",
            "→ Weigh-In wi-abc123: decided --> <b>x</b> (2026-10-07)",
        ] {
            for syntax in [Syntax::Code, Syntax::Prose] {
                let out = annotate_line("// TODO: pick a cap", note, syntax);

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
            let out = annotate_line(line, NOTE, Syntax::Code);

            assert_eq!(out, Ok(format!("{line} {NOTE}")), "{line}");
        }
    }

    #[test]
    fn a_hash_inside_code_does_not_open_a_comment() {
        let out = annotate_line(
            r##"    let s = r#"// TODO: x"#; f(s);"##,
            NOTE,
            Syntax::Code,
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
            let out = annotate_line(line, NOTE, Syntax::Code);

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
            let out = annotate_line(line, note, Syntax::Code);

            assert_eq!(out, Err(Skip::UnsafeText), "{line}");
        }
    }

    #[test]
    fn a_note_with_an_opener_still_fits_a_line_comment() {
        let note = "→ Weigh-In wi-abc123: decided a /* b --force (2026-10-07)";

        let out = annotate_line("// TODO: pick", note, Syntax::Code);

        assert_eq!(out, Ok(format!("// TODO: pick {note}")));
    }
}
