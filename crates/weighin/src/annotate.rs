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
//! marker stands after code with nothing after it to confirm it. Because the tokeniser ([`lex`])
//! cannot know every language's literals (lifetimes, digit separators, triple quotes, regex
//! literals, verbatim strings), a guard ([`guard`]) that does not tokenise has the last word: a
//! comment whose quotes may pair otherwise, a line comment holding PHP's `?>`, or a `//` after a
//! JSX tag is left alone too.

mod guard;
mod lang;
mod lex;

pub(crate) use lang::Lang;
use lex::{Found, opener_at, scan_code};

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

/// Block comment closers: text after one is code again.
const CLOSERS: &[&str] = &["*/", "-->"];

/// Whether `c` could end a line or a line comment: a control character, or the Unicode line
/// and paragraph separators, which end a `//` comment in JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
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
/// marker inside a string, the line is left alone. Before writing, [`guard::check`] refuses a
/// comment the tokeniser may have found by pairing quotes wrongly; it checks from the first line
/// opener, where the comment begins under the line comment reading.
fn annotate_code(line: &str, note: &str, lang: Lang) -> Result<String, Skip> {
    let mut from = 0;
    let mut marker_in_string = false;
    let mut closed_one = false;
    // The first line opener, if one was found: under the line comment reading, the comment the
    // note goes in starts there.
    let mut first_line: Option<Found> = None;
    // Whether a comment the line ends in under one reading opens the line, so no reading of a
    // file of unknown language takes it for code.
    let mut certain = lang.known();
    loop {
        let stretch = scan_code(line, from, lang);
        marker_in_string |= stretch.marker_in_string;
        let Some(found) = stretch.comment else {
            return if closed_one {
                Err(Skip::CodeAfterComment)
            } else if marker_in_string || (first_line.is_some() && stretch.open_string) {
                Err(Skip::InString)
            } else if first_line.is_some() && line.trim_end().ends_with('\\') {
                // A backslash splices the next line into the comment; the note would end that.
                Err(Skip::Spliced)
            } else if let Some(first) = first_line
                && certain
            {
                // Read as code, the rest holds no comment: only the line comment reading stands.
                guard::check(line, first, lang)?;
                Ok(format!("{line} {note}"))
            } else if first_line.is_some() {
                Err(Skip::Ambiguous)
            } else {
                Err(Skip::NoComment)
            };
        };
        let Some(closer) = found.kind.closer() else {
            // Read the rest as code too, afresh: what came before the opener is settled.
            certain |= opens_line(line, found.start);
            first_line = first_line.or(Some(found));
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
        guard::check(line, first_line.unwrap_or(found), lang)?;
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
        (
            "python",
            "help = '''Don't use # TODO: explain'''",
            Want::Skipped(Skip::InString),
        ),
        (
            "python",
            "x = '''don't # TODO: explain",
            Want::Skipped(Skip::InString),
        ),
        (
            "js",
            "s.replace(/'/g, '// TODO: x')",
            Want::Skipped(Skip::InString),
        ),
        (
            "js",
            "s.replace(/\"/g, \"x // TODO: y\").replace(/\"/g, \"\")",
            Want::Skipped(Skip::InString),
        ),
        (
            "js",
            "s.replace(/`/g, `x // TODO: y",
            Want::Skipped(Skip::Ambiguous),
        ),
        (
            "rust",
            "let v: &'a str = pick(\"' // TODO: decide\", '\"');",
            Want::Skipped(Skip::InString),
        ),
        (
            "rust",
            "let v: &'a str = \"' // TODO: decide",
            Want::Skipped(Skip::InString),
        ),
        (
            "c",
            "int x = 1'000; const char* s = \"' // TODO: q\"; char c = '\"';",
            Want::Skipped(Skip::InString),
        ),
        (
            "c",
            "var p = @\"C:\\\"; Log(\"// TODO: x\" + '\"');",
            Want::Skipped(Skip::InString),
        ),
        (
            "php",
            "<?php // TODO: pick ?>",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "unknown",
            "    // TODO: pick ?> <p>",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "jsx",
            "  return <p>// TODO: x</p>;",
            Want::Skipped(Skip::Ambiguous),
        ),
        ("rust", "    // TODO: don't cache it", Want::End),
        ("js", "const re = /a/g; // TODO: pick a flag", Want::End),
        ("php", "$x = 1; // TODO: pick a cap", Want::End),
        ("jsx", "  // TODO: pick a prop", Want::End),
        ("jsx", "const f = () => a > b; // TODO: pick", Want::End),
    ];

    /// The language a corpus row names.
    fn corpus_lang(name: &str) -> Lang {
        match name {
            "rust" => Lang::Rust,
            "c" | "js" => Lang::CFamily,
            "python" | "shell" | "yaml" | "dockerfile" => Lang::Hash,
            "php" => Lang::HashSlash,
            "jsx" => Lang::Jsx,
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
