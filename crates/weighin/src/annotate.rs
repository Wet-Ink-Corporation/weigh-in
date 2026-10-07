//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A code line is only ever changed inside its last comment: the note goes at the end of a line
//! comment, or just before the closer of a block comment that ends the line. Comment markers
//! inside a string literal do not count; `/*` glued to a word (`build/**/*.o`) does not open a
//! comment; and `#`, `--` and `REM ` open one only standing alone (at the line start or after
//! whitespace, and `#` and `--` not before a word character), so `r#"`, `x--` and `#define` stay
//! code. A line opener can still be code (`http://`, Python's `a // b`, `#  define`, `i --`), so
//! the rest of the line is also read as code, and the note goes only where both readings keep
//! it in a comment: before the closer of a block comment that ends the line, or at the end of
//! the line. Anything else is left as it was, so a decision does not break the code around its
//! question. The reading is a line-local heuristic, not a parser: a line inside a string or
//! comment opened on an earlier line is read as if it started in code; a line comment glued to
//! its marker (`#TODO`) is left alone; and so is a line comment whose text, read as code, holds
//! a comment marker inside quotes or closes a block comment before more text.

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
    /// Anywhere outside a string literal, but not glued to a word character before it: so the
    /// `/*` of a path or glob (`build/**/*.o`) stays text.
    AfterNonWord,
    /// Only at the start of the line, after indentation (a block comment's middle `*`).
    LineStart,
    /// At the start of the line or after whitespace, and, unless the opener ends in a space
    /// (`REM `), not followed by a word character, `[` or `!`: so `r#"`, `x--`, `#define`,
    /// `#[derive]` and `--count` stay code.
    Standalone,
}

/// The comment openers `weighin-core`'s TODO scan recognises, so every line it raised from is
/// read the same way here, longest first, with the comment each opens and where it may stand.
/// The scan's `'` is read in context instead ([`scan_code`]).
const OPENERS: &[(&str, Comment, Place)] = &[
    ("<!--", Comment::Html, Place::Anywhere),
    ("/*", Comment::Star, Place::AfterNonWord),
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

/// Whether `c` belongs to a word: a letter, a digit or `_`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `opener`, which `tail` starts with and `before` precedes, stands where `place` allows.
fn stands(opener: &str, place: Place, before: &str, tail: &str) -> bool {
    match place {
        Place::Anywhere => true,
        Place::AfterNonWord => !before.chars().next_back().is_some_and(is_word),
        Place::LineStart => before.trim().is_empty(),
        Place::Standalone => {
            let next = tail.get(opener.len()..).and_then(|t| t.chars().next());
            after_space(before)
                && (opener.ends_with(' ')
                    || !next.is_some_and(|c| is_word(c) || matches!(c, '[' | '!')))
        }
    }
}

/// The comment opener at byte `at` of `line`, if one starts there: its length and kind.
fn opener_at(line: &str, at: usize) -> Option<(usize, Comment)> {
    let tail = line.get(at..)?;
    let before = line.get(..at)?;
    OPENERS
        .iter()
        .find(|(m, _, place)| tail.starts_with(m) && stands(m, *place, before, tail))
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
/// starts the line and no other opener follows it on the line; otherwise it is an ordinary
/// character (a Rust lifetime or an OCaml type variable).
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
                None if line.get(..at).is_some_and(after_space)
                    && !opener_follows(line, at.saturating_add(1)) =>
                {
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

/// Whether `c` could end a line or a line comment: a control character, or the Unicode line
/// and paragraph separators, which end a `//` comment in JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Whether a comment opener starts anywhere in `line` from byte `from` on.
fn opener_follows(line: &str, from: usize) -> bool {
    line.get(from..).is_some_and(|tail| {
        tail.char_indices()
            .any(|(i, _)| opener_at(line, from.saturating_add(i)).is_some())
    })
}

/// `line` with `note` written into its comment, or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end or change the comment, or end the line;
/// [`Skip::NoComment`] for a code line with no comment; [`Skip::InString`] when its only comment
/// markers, or those in a line comment's text read as code, sit inside a string;
/// [`Skip::CodeAfterComment`] when its last comment, or one in a line comment's text read as
/// code, closes before the end of the line.
pub(crate) fn annotate_line(line: &str, note: &str, syntax: Syntax) -> Result<String, Skip> {
    if note.chars().any(ends_a_line) || CLOSERS.iter().any(|c| note.contains(c)) {
        return Err(Skip::UnsafeText);
    }
    match syntax {
        Syntax::Prose => Ok(format!("{line} {note}")),
        Syntax::Code => annotate_code(line, note),
    }
}

/// [`annotate_line`] for a code line: walks its comments in order and writes into the last one,
/// if the line ends inside it or with its closer.
///
/// A line opener may itself be code (`http://`, Python's `a // b`, `#  define`, `i --`), so the
/// rest of the line is read twice: as the comment's body and as code. The note goes where both
/// readings allow it: before the closer of a block comment that ends the line, or at the end of
/// the line when the code reading ends in a comment or finds none. When the code reading closes
/// a comment before more code, or finds a marker inside a string, the line is left alone.
fn annotate_code(line: &str, note: &str) -> Result<String, Skip> {
    let mut from = 0;
    let mut marker_in_string = false;
    let mut closed_one = false;
    let mut line_opened = false;
    loop {
        let stretch = scan_code(line, from);
        marker_in_string |= stretch.marker_in_string;
        let Some((body_at, kind)) = stretch.comment else {
            return if closed_one {
                Err(Skip::CodeAfterComment)
            } else if marker_in_string {
                Err(Skip::InString)
            } else if line_opened {
                // Read as code, the rest holds no comment: only the line comment reading stands.
                Ok(format!("{line} {note}"))
            } else {
                Err(Skip::NoComment)
            };
        };
        let Some(closer) = kind.closer() else {
            // Read the rest as code too, afresh: what came before the opener is settled.
            line_opened = true;
            closed_one = false;
            marker_in_string = false;
            from = body_at;
            continue;
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
            "→ Weigh-In wi-abc123: decided a\u{2028}alert(1); (2026-10-07)",
            "→ Weigh-In wi-abc123: decided a\u{2029}alert(1); (2026-10-07)",
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

    #[test]
    fn a_lifetime_before_a_block_comment_does_not_open_a_comment() {
        let ok = annotate_line("fn f<T: 'static>() {} /* TODO: x */", NOTE, Syntax::Code);
        let skipped = annotate_line(
            "impl<T: 'static> X for T { /* TODO: x */ }",
            NOTE,
            Syntax::Code,
        );

        assert_eq!(
            ok,
            Ok(format!("fn f<T: 'static>() {{}} /* TODO: x {NOTE} */"))
        );
        assert_eq!(skipped, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn code_after_a_comment_is_the_reason_even_when_a_later_string_holds_a_marker() {
        let out = annotate_line(r#"/* TODO: x */ s = "//";"#, NOTE, Syntax::Code);

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
            let out = annotate_line(line, NOTE, Syntax::Code);

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
            let out = annotate_line(line, NOTE, Syntax::Code);

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
            let out = annotate_line(line, NOTE, Syntax::Code);

            assert_eq!(out, Err(want), "{line}");
        }
    }

    #[test]
    fn rem_opens_a_comment_at_the_line_start() {
        let out = annotate_line("REM TODO: pick a drive", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("REM TODO: pick a drive {NOTE}")));
    }
}
