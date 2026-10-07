//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the decision in, the rewritten line or the reason it was left alone out.
//!
//! A prose line (markdown, plain text) gets the note at its end. A code line gets it only when
//! a comment is the whole line, read in the file's language ([`Lang`], from its name):
//! - a line that starts with one of the language's line comment openers (not a `#!` shebang)
//!   gets it at the end, unless the line holds PHP's `?>`, ends in a `\` line splice, or
//!   already holds a line break (a lone CR, U+0085, U+2028 or U+2029);
//! - a line that is exactly one block comment, `/* … */` or `<!-- … -->`, with nothing before
//!   its opener or after its closer, gets it just before the closer.
//!
//! Anything else is left as it was, a comment after code (`x = 1  # TODO`) included.
//!
//! The reading is per line, so a whole-line comment may sit inside a string or block comment
//! opened on an earlier line. Three refusals keep the note from closing or breaking one: a line
//! holding a closer of a string or comment ([`may_close`]), an option holding anything but a
//! few safe characters ([`safe_option`]), and a file that may need ASCII where the note goes
//! ([`file_fits_note`]). A string with a delimiter of its own (Perl's `q,…,`) may still end on
//! an allowed character: that residual risk is the cost of the per-line rule.

mod lang;

use lang::Block;
pub(crate) use lang::Lang;

/// How the source file reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    /// Prose (markdown, plain text and the like): the note goes at the end of the line.
    Prose,
    /// Anything else: the note goes only into a comment that is the whole line.
    Code,
}

/// How a file reads when a decision is written back into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Prose or code.
    pub(crate) syntax: Syntax,
    /// The comment syntax a code file is read in.
    pub(crate) lang: Lang,
}

/// The decision a note records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Decision<'a> {
    /// The item's `wi-` id.
    pub(crate) id: &'a str,
    /// The option decided, as the owner wrote it.
    pub(crate) option: &'a str,
    /// The day it was decided, `YYYY-MM-DD`.
    pub(crate) date: &'a str,
}

impl Decision<'_> {
    /// `→ Weigh-In <id>: decided <option> (<date>)`.
    fn note(&self) -> String {
        format!(
            "→ Weigh-In {}: decided {} ({})",
            self.id, self.option, self.date
        )
    }
}

/// Why a source line was left as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The option holds a line break (a control character, U+2028 or U+2029), or `--`, which
    /// XML does not allow inside `<!-- -->`.
    UnsafeText,
    /// The line is not a comment of the file's language from its start: code, a string, a
    /// shebang or a block comment's middle line may hold the marker.
    NotWholeLine,
    /// Code follows the comment on the line: after a block comment's closer, or after PHP's `?>`.
    CodeAfterComment,
    /// The comment runs on to the next line: a line comment ending in a `\` line splice, or a
    /// block comment that does not close on the line.
    RunsOn,
    /// The block comment holds another opener, which nests in some languages.
    Nested,
    /// In code, the option holds a character outside the allowed set (see [`safe_option`]),
    /// which could end the comment or end or change a string around it.
    MayCloseString,
    /// The line holds text that could close a string, template, JSX text or block comment
    /// opened on an earlier line (see [`may_close`]), after which the note would be code.
    MayCloseAbove,
    /// The code file may need ASCII where the note's `→` would go (see [`file_fits_note`]).
    AsciiOnly,
}

impl Skip {
    /// The reason, as a receipt shows it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsafeText => "the option does not fit on one comment line",
            Self::NotWholeLine => "the comment is not the whole line",
            Self::CodeAfterComment => "code follows the comment",
            Self::RunsOn => "the comment runs on to the next line",
            Self::Nested => "the comment holds another comment opener",
            Self::MayCloseString => {
                "the option holds a character that could end the comment or a string around it"
            }
            Self::MayCloseAbove => "the line may close a string or comment opened above",
            Self::AsciiOnly => "the file may hold only ASCII where the note would go",
        }
    }
}

/// The characters that could end a string, template or JSX text in any language: quotes,
/// backticks, `${`/`{` interpolation braces and JSX's tag brackets (which cover `-->` and `?>`).
const STRING_CLOSERS: [char; 6] = ['"', '`', '{', '}', '<', '>'];

/// The texts that close a block comment or long string: `*/`, Lua's `]]` and `]=`, Julia's
/// `=#`, Nim's `]#` and CoffeeScript's `###`.
const OTHER_CLOSERS: [&str; 6] = ["*/", "]]", "]=", "=#", "]#", "###"];

/// Whether a comment line of a file in `lang`, `text` as a whole and `body` after its opener,
/// could close a string, template, JSX text or block comment opened on an earlier line:
/// [`STRING_CLOSERS`] or [`OTHER_CLOSERS`] in `text`, or one of the language's own
/// [`Lang::closers`] in `body`.
fn may_close(text: &str, body: &str, lang: Lang) -> bool {
    text.contains(STRING_CLOSERS)
        || OTHER_CLOSERS.iter().any(|c| text.contains(c))
        || body.contains(lang.closers())
}

/// Whether every character of `option` may be written into a comment of a file in `lang`: a
/// letter, a digit, a space or one of `. , : ; - _ + = / ! ? '`, and `%` where batch files do
/// not expand it; never one of the language's [`Lang::closers`]. So no sigil, bracket, quote or
/// backslash can interpolate into or close a string the comment may sit in.
fn safe_option(option: &str, lang: Lang) -> bool {
    option.chars().all(|c| {
        let allowed = c.is_alphanumeric()
            || matches!(
                c,
                ' ' | '.' | ',' | ':' | ';' | '-' | '_' | '+' | '=' | '/' | '!' | '?' | '\''
            )
            || (c == '%' && !lang.expands_percent());
        allowed && !lang.closers().contains(&c)
    })
}

/// Whether `c` could end a line or a line comment: a control character (`\r` and U+0085
/// included), or the Unicode line and paragraph separators, which end a `//` comment in
/// JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Whether a note may go anywhere in `text`, a whole file read as `reading`: always in prose;
/// in code only when nothing in it may need ASCII where the note's `→` lands.
///
/// # Errors
/// [`Skip::AsciiOnly`] when the code may hold a byte string ([`holds_byte_string`]), or its
/// first two lines declare an encoding other than UTF-8 ([`declares_other_coding`]).
pub(crate) fn file_fits_note(text: &str, reading: Reading) -> Result<(), Skip> {
    match reading.syntax {
        Syntax::Code if holds_byte_string(text) || declares_other_coding(text) => {
            Err(Skip::AsciiOnly)
        }
        Syntax::Code | Syntax::Prose => Ok(()),
    }
}

/// Whether `text` holds what may open a byte string, which takes only ASCII: `b'`, `b"`, or
/// `br`/`rb` before a quote (`br#` for Rust), in any case, not glued to a word before it.
fn holds_byte_string(text: &str) -> bool {
    const PREFIXES: [&str; 7] = ["b'", "b\"", "br'", "br\"", "br#", "rb'", "rb\""];
    let mut prev: Option<char> = None;
    text.char_indices().any(|(at, c)| {
        let glued = prev.is_some_and(|p| p.is_alphanumeric() || p == '_');
        prev = Some(c);
        !glued
            && text
                .get(at..)
                .is_some_and(|rest| starts_with_any(rest, &PREFIXES))
    })
}

/// Whether one of the first two lines of `text` declares an encoding other than UTF-8, as
/// Python's PEP 263 and Ruby read `coding: <name>` or `coding=<name>`.
fn declares_other_coding(text: &str) -> bool {
    text.lines().take(2).any(|line| {
        line.split("coding").skip(1).any(|rest| {
            rest.strip_prefix([':', '='])
                .and_then(|r| {
                    r.trim_start()
                        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
                        .next()
                })
                .is_some_and(|name| {
                    !name.is_empty() && !starts_with_any(name, &["utf-8", "utf8", "utf_8"])
                })
        })
    })
}

/// Whether `text` starts with one of `prefixes`, in any ASCII case.
fn starts_with_any(text: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| {
        text.get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    })
}

/// `line` of a file read as `reading`, with the note of `decision` written into it, or why it
/// cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the option could end the line; [`Skip::MayCloseString`] when, in
/// code, it is not [`safe_option`]; for a code line, the [`Skip`] that says
/// why its comment is not the whole line or could not hold the note.
pub(crate) fn annotate_source_line(
    line: &str,
    decision: &Decision<'_>,
    reading: Reading,
) -> Result<String, Skip> {
    if decision.option.chars().any(ends_a_line) {
        return Err(Skip::UnsafeText);
    }
    match reading.syntax {
        Syntax::Prose => Ok(format!("{line} {}", decision.note())),
        Syntax::Code => annotate_code(line, decision, reading.lang),
    }
}

/// [`annotate_source_line`] for a code line in `lang`.
fn annotate_code(line: &str, decision: &Decision<'_>, lang: Lang) -> Result<String, Skip> {
    if !safe_option(decision.option, lang) {
        return Err(Skip::MayCloseString);
    }
    let trimmed = line.trim();
    if let Some(&block) = lang.blocks().iter().find(|b| trimmed.starts_with(b.open)) {
        return annotate_block(line, decision, block, lang);
    }
    let body = lang.line_comment_body(trimmed).ok_or(Skip::NotWholeLine)?;
    if trimmed.contains("?>") {
        return Err(Skip::CodeAfterComment);
    }
    if trimmed.ends_with('\\') || line.chars().any(|c| c != '\t' && ends_a_line(c)) {
        return Err(Skip::RunsOn);
    }
    if may_close(trimmed, body, lang) {
        return Err(Skip::MayCloseAbove);
    }
    Ok(format!("{line} {}", decision.note()))
}

/// `line`, which starts (after indentation) with `block`'s opener, with the note of `decision`
/// written just before its closer, when that closer ends the line, the comment holds no other
/// opener and nothing in it could close a construct opened above.
fn annotate_block(
    line: &str,
    decision: &Decision<'_>,
    block: Block,
    lang: Lang,
) -> Result<String, Skip> {
    if block
        .option_forbids
        .is_some_and(|forbidden| decision.option.contains(forbidden))
    {
        return Err(Skip::UnsafeText);
    }
    let indent = line.len().saturating_sub(line.trim_start().len());
    let body_at = indent.saturating_add(block.open.len());
    let body = line.get(body_at..).ok_or(Skip::NotWholeLine)?;
    let Some(at) = body.find(block.close) else {
        return Err(Skip::RunsOn);
    };
    let (inner, closing) = body.split_at_checked(at).ok_or(Skip::NotWholeLine)?;
    let after = closing.get(block.close.len()..).ok_or(Skip::NotWholeLine)?;
    if !after.trim().is_empty() {
        return Err(Skip::CodeAfterComment);
    }
    if inner.contains(block.open) {
        return Err(Skip::Nested);
    }
    if may_close(inner, inner, lang) {
        return Err(Skip::MayCloseAbove);
    }
    let head = line
        .get(..body_at.saturating_add(at))
        .ok_or(Skip::NotWholeLine)?;
    Ok(format!("{} {} {closing}", head.trim_end(), decision.note()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "→ Weigh-In wi-abc123: decided 4096 (2026-10-07)";

    /// The decision of `option` on wi-abc123, on 2026-10-07.
    fn decision(option: &str) -> Decision<'_> {
        Decision {
            id: "wi-abc123",
            option,
            date: "2026-10-07",
        }
    }

    /// `line`, read as `syntax` in `lang`, with the decision of `option` written into it.
    fn write(syntax: Syntax, lang: Lang, line: &str, option: &str) -> Result<String, Skip> {
        annotate_source_line(line, &decision(option), Reading { syntax, lang })
    }

    /// `line` of the file `file`, read as code, with [`NOTE`] written into it.
    fn code(file: &str, line: &str) -> Result<String, Skip> {
        write(Syntax::Code, Lang::of(file), line, "4096")
    }

    #[test]
    fn a_line_comment_gets_the_note_at_the_end() {
        let out = code("a.rs", "    // TODO: pick a cap");

        assert_eq!(out, Ok(format!("    // TODO: pick a cap {NOTE}")));
    }

    #[test]
    fn a_block_comment_alone_gets_the_note_before_its_closer() {
        let out = code("a.c", "    /* TODO: pick a timeout */");

        assert_eq!(out, Ok(format!("    /* TODO: pick a timeout {NOTE} */")));
    }

    #[test]
    fn an_html_comment_alone_gets_the_note_before_its_closer() {
        let out = code("a.html", "<!-- TODO: choose a layout -->  ");

        assert_eq!(out, Ok(format!("<!-- TODO: choose a layout {NOTE} -->  ")));
    }

    #[test]
    fn a_markdown_line_gets_the_note_at_the_end() {
        let out = write(
            Syntax::Prose,
            Lang::Unknown,
            "- Should we shard by tenant?",
            "4096",
        );

        assert_eq!(out, Ok(format!("- Should we shard by tenant? {NOTE}")));
    }

    #[test]
    fn a_trailing_comment_after_code_is_left_alone() {
        for (file, line) in [
            ("a.py", "x = 1  # TODO: pick a start"),
            ("a.c", "    *p = 0; // TODO: pick a default"),
            ("a.c", "int x; /* TODO: pick a width */"),
        ] {
            assert_eq!(code(file, line), Err(Skip::NotWholeLine), "{line}");
        }
    }

    #[test]
    fn a_block_comment_followed_by_code_is_left_alone() {
        let out = code("a.c", "    /* TODO: pick a buffer size */ return 0;");

        assert_eq!(out, Err(Skip::CodeAfterComment));
    }

    #[test]
    fn a_block_comment_that_runs_past_the_line_is_left_alone() {
        let out = code("a.c", "/* TODO: pick a cap, because");

        assert_eq!(out, Err(Skip::RunsOn));
    }

    #[test]
    fn a_block_comment_holding_another_opener_is_left_alone() {
        let out = code("a.rs", "/* TODO: see /* the old cap */");

        assert_eq!(out, Err(Skip::Nested));
    }

    #[test]
    fn an_option_with_a_line_break_is_refused_in_code_and_prose() {
        for option in ["10\nreturn 99;", "a\u{2028}b", "a\tb", "a\rb"] {
            assert_eq!(
                [Syntax::Code, Syntax::Prose].map(|syntax| write(
                    syntax,
                    Lang::CFamily,
                    "// TODO: pick a cap",
                    option
                )),
                [Err(Skip::UnsafeText), Err(Skip::UnsafeText)],
                "{option:?}"
            );
        }
    }

    #[test]
    fn an_option_with_dashes_is_refused_inside_an_html_comment() {
        let out = write(Syntax::Code, Lang::Markup, "<!-- TODO: pick -->", "a -- b");

        assert_eq!(out, Err(Skip::UnsafeText));
    }

    #[test]
    fn a_code_option_outside_the_safe_characters_is_refused() {
        for option in [
            "x */ y",
            "x --> y",
            "x ?> y",
            "a /* b",
            r#"x """ y"#,
            "a ` b",
            "a ${ b",
            "f{x}",
            r##"a "# b"##,
            r#"a )" b"#,
            "a ]] b",
            "a > b",
            "a < b",
            "a ''' b",
            r"C:\N",
            r"use \x1",
            "$a[",
            "$(nproc",
            "$((",
            "a)",
            "me@example",
            "#{x}",
            "a =# b",
            "a\u{202e}b",
        ] {
            let out = write(Syntax::Code, Lang::CFamily, "// TODO: pick a cap", option);

            assert_eq!(out, Err(Skip::MayCloseString), "{option:?}");
        }
    }

    #[test]
    fn a_code_option_of_letters_digits_and_safe_punctuation_is_written() {
        let option = "Größe 4k, max: 50% - a_b + c = d/e! ok? ja; 1.5";

        let out = write(Syntax::Code, Lang::CFamily, "// TODO: pick", option);

        assert_eq!(
            out,
            Ok(format!(
                "// TODO: pick → Weigh-In wi-abc123: decided {option} (2026-10-07)"
            ))
        );
    }

    #[test]
    fn a_percent_is_refused_where_batch_files_may_expand_it() {
        let read = |lang: Lang, line: &str| write(Syntax::Code, lang, line, "50%").map(|_| ());

        assert_eq!(
            [
                read(Lang::Batch, "REM TODO: pick"),
                read(Lang::Unknown, "REM TODO: pick"),
                read(Lang::CFamily, "// TODO: pick"),
            ],
            [Err(Skip::MayCloseString), Err(Skip::MayCloseString), Ok(())]
        );
    }

    #[test]
    fn a_code_option_with_an_apostrophe_fits_only_where_a_quote_cannot_close_a_string() {
        let note = "→ Weigh-In wi-abc123: decided don't cache (2026-10-07)";
        let read = |lang: Lang, line: &str| write(Syntax::Code, lang, line, "don't cache");

        assert_eq!(
            [
                read(Lang::Basic, "' TODO: cache?"),
                read(Lang::Batch, "REM TODO: cache?"),
                read(Lang::CFamily, "// TODO: cache?"),
                read(Lang::HashSlash, "# TODO: cache?"),
                read(Lang::Hash, "# TODO: cache?"),
                read(Lang::Sql, "-- TODO: cache?"),
                read(Lang::Dashes, "-- TODO: cache?"),
                read(Lang::Unknown, "# TODO: cache?"),
            ],
            [
                Ok(format!("' TODO: cache? {note}")),
                Ok(format!("REM TODO: cache? {note}")),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
            ]
        );
    }

    #[test]
    fn a_slash_or_bang_is_refused_where_a_string_may_take_it_as_its_delimiter() {
        let read = |lang: Lang, line: &str, option: &str| {
            write(Syntax::Code, lang, line, option).map(|_| ())
        };

        assert_eq!(
            [
                read(Lang::Hash, "# TODO: pick a cap", "a/b"),
                read(Lang::Hash, "# TODO: pick a cap", "now!"),
                read(Lang::Unknown, "# TODO: pick a cap", "a/b"),
                read(Lang::CFamily, "// TODO: pick a cap", "a/b!"),
            ],
            [
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Ok(()),
            ]
        );
    }

    #[test]
    fn a_code_file_that_may_hold_only_ascii_where_the_note_goes_is_refused() {
        let code = |text: &str| {
            file_fits_note(
                text,
                Reading {
                    syntax: Syntax::Code,
                    lang: Lang::Unknown,
                },
            )
        };

        for text in [
            "data = b'''\n# TODO: pick a cap\n'''\n",
            "data = B\"\"\"\n# TODO: pick a cap\n\"\"\"\n",
            "let s = b\"\n// TODO: pick a cap\n\";\n",
            "let s = br#\"\n// TODO: pick a cap\n\"#;\n",
            "x = (rb'a\n# TODO: pick a cap\n')\n",
            "# -*- coding: ascii -*-\n# TODO: pick a cap\n",
            "#!/usr/bin/env python\n# vim: set fileencoding=latin-1 :\n# TODO: pick\n",
            "# encoding: us-ascii\n# TODO: pick a cap\n",
        ] {
            assert_eq!(code(text), Err(Skip::AsciiOnly), "{text:?}");
        }
        for text in [
            "# TODO: pick a cap\n",
            "# -*- coding: utf-8 -*-\n# TODO: pick a cap\n",
            "sub = \"a\"\nverb'x\n// TODO: pick\n",
            "\n\n# coding: ascii\n# TODO: pick a cap\n",
        ] {
            assert_eq!(code(text), Ok(()), "{text:?}");
        }
        let prose = Reading {
            syntax: Syntax::Prose,
            lang: Lang::Unknown,
        };
        assert_eq!(file_fits_note("x = b'a'\n- Shard?\n", prose), Ok(()));
    }

    #[test]
    fn a_prose_option_may_hold_comment_closers_and_quotes() {
        let option = r#""a */ b --> c ?>" <d> $(e) @f \g"#;

        let out = write(Syntax::Prose, Lang::Unknown, "- Shard?", option);

        assert_eq!(
            out,
            Ok(format!(
                "- Shard? → Weigh-In wi-abc123: decided {option} (2026-10-07)"
            ))
        );
    }

    /// Where the note should go in a corpus line: at the end, before the line's closer, or
    /// nowhere (left alone, for the given reason).
    #[derive(Debug, Clone, Copy)]
    enum Want {
        End,
        BeforeCloser(&'static str),
        Skipped(Skip),
    }

    /// Real code lines, among them every adversarial line reviews found, with the file they come
    /// from and where the note should go.
    const CORPUS: &[(&str, &str, Want)] = &[
        ("a.rs", "    // TODO: pick a cap", Want::End),
        (
            "a.rs",
            "    // TODO: don't cache it",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.rs", "/// TODO: document the cap", Want::End),
        (
            "a.c",
            "    /* TODO: pick a scale */",
            Want::BeforeCloser("*/"),
        ),
        ("a.c", "/** TODO: pick a scale */", Want::BeforeCloser("*/")),
        ("a.css", "/* TODO: pick a size */", Want::BeforeCloser("*/")),
        ("a.sql", "-- TODO: pick a dash", Want::End),
        (
            "a.sql",
            "/* TODO: pick a limit */",
            Want::BeforeCloser("*/"),
        ),
        (
            "a.html",
            "<!-- TODO: choose a layout -->",
            Want::BeforeCloser("-->"),
        ),
        ("a.py", "# TODO: pick a divisor", Want::End),
        ("a.py", "  #TODO: glued", Want::End),
        ("ci.yml", "  # TODO: pick a count", Want::End),
        ("Dockerfile", "# TODO: pick a base", Want::End),
        ("init.lua", "-- TODO: pick a step", Want::End),
        ("a.vb", "' TODO: pick a start", Want::End),
        ("a.vb", "REM TODO: pick a start", Want::End),
        ("a.bat", "REM TODO: pick a drive", Want::End),
        ("a.php", "# TODO: pick a cap", Want::End),
        ("a.php", "// TODO: pick a cap", Want::End),
        ("a.jsx", "  // TODO: pick a prop", Want::End),
        ("run", "# TODO: a // b", Want::Skipped(Skip::MayCloseAbove)),
        (
            "run",
            "    // TODO: see http://x",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "a.py",
            "# TODO: see http://x",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.c", "// TODO: see http://x", Want::End),
        ("run", "REM TODO: pick a drive", Want::End),
        ("run", "/* TODO: pick */", Want::BeforeCloser("*/")),
        // Trailing comments and markers that are code or text: left alone.
        (
            "a.py",
            "x = a // b  # TODO: pick a divisor",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.py",
            "half = len(items) // 2 if cache else 0",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.c",
            "#define X 1 // TODO: q9 word9 \\",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.c",
            "x = y -- TODO_count;",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.c",
            " * TODO: pick a width",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.c",
            "    *p = 0; // TODO: pick a default",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.css",
            "// TODO: not a CSS comment",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.html",
            "<p>x</p> <!-- TODO: x -->",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.php",
            "#[Route(\"/todo\")]",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "init.lua",
            "--[[ TODO: x ]] y = 1",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "get.sh",
            "curl -s https://x/todo/items | jq .",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "get.sh",
            "echo ' TODO: pick",
            Want::Skipped(Skip::NotWholeLine),
        ),
        ("run", "#define TODO 1", Want::Skipped(Skip::NotWholeLine)),
        (
            "run",
            "x = 1 ' TODO: pick a start",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "run",
            "ls /* # TODO: pick a glob",
            Want::Skipped(Skip::NotWholeLine),
        ),
        // Review F1-F5: Python triple quote, JS regex with a quote, Rust lifetime, C++ digit
        // separator, C# verbatim string, PHP `?>` and JSX text.
        (
            "a.py",
            "help = '''Don't use # TODO: explain'''",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.py",
            "x = '''don't # TODO: explain",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.js",
            "s.replace(/'/g, '// TODO: x')",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.js",
            "s.replace(/`/g, `x // TODO: y",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.js",
            "const re = /a/g; // TODO: pick a flag",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.rs",
            "fn f<'a>(x: &'a str) {} // TODO: pick",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.rs",
            "fn f(x: impl Fn() + 'static) { todo!(\"TODO: x\") }",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.rs",
            "let v: &'a str = pick(\"' // TODO: decide\", '\"');",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.rs",
            "    let s = r#\"// TODO: x\"#; f(s);",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.cpp",
            "int x = 1'000; const char* s = \"' // TODO: q\"; char c = '\"';",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.cs",
            "var p = @\"C:\\\"; Log(\"// TODO: x\" + '\"');",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.php",
            "<?php // TODO: pick ?>",
            Want::Skipped(Skip::NotWholeLine),
        ),
        (
            "a.php",
            "// TODO: pick ?> <p>",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "run",
            "    // TODO: pick ?> <p>",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "a.jsx",
            "  return <p>// TODO: x</p>;",
            Want::Skipped(Skip::NotWholeLine),
        ),
        // Comments that end or run on early.
        ("a.c", "// TODO: q9 word9 \\", Want::Skipped(Skip::RunsOn)),
        (
            "a.c",
            "/* a */ int x; // TODO: pick a width",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "a.c",
            "/* TODO: x */ s = \"//\";",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        (
            "a.html",
            "<!-- a --> <!-- TODO: x -->",
            Want::Skipped(Skip::CodeAfterComment),
        ),
        ("a.c", "/*/ TODO: pick", Want::Skipped(Skip::RunsOn)),
        // Review round 9, F1: a line comment that already holds a line break (JavaScript's
        // U+2028, Python's lone CR, C#'s U+0085) ends there; the rest of the line is code.
        (
            "a.js",
            "// TODO: q \u{2028} let x=1;",
            Want::Skipped(Skip::RunsOn),
        ),
        ("a.py", "# TODO: q\rx = 1", Want::Skipped(Skip::RunsOn)),
        (
            "a.cs",
            "// TODO: q\u{85}int x;",
            Want::Skipped(Skip::RunsOn),
        ),
        ("a.c", "//\tTODO: pick a tab", Want::End),
        ("a.js", "/* TODO: q \u{2028} x */", Want::BeforeCloser("*/")),
        // Review round 10, F1: a whole-line comment inside a block comment, docstring,
        // template, raw string or quoted string opened above, on a line that closes that
        // construct: the note would land in code after the closer.
        (
            "a.c",
            "// TODO: pick a width */",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.php", "# TODO: x */", Want::Skipped(Skip::MayCloseAbove)),
        (
            "a.py",
            "    # TODO: pick a cap\"\"\"",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.js", "// TODO: x`;", Want::Skipped(Skip::MayCloseAbove)),
        ("a.js", "// TODO: x ${", Want::Skipped(Skip::MayCloseAbove)),
        (
            "a.jsx",
            "// TODO: x</p>;",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "a.cpp",
            "// TODO: x)\";",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.c", "// TODO: x\";", Want::Skipped(Skip::MayCloseAbove)),
        ("a.sh", "# TODO: x'", Want::Skipped(Skip::MayCloseAbove)),
        ("a.sh", "# TODO: x\"", Want::Skipped(Skip::MayCloseAbove)),
        ("a.sql", "-- TODO: x';", Want::Skipped(Skip::MayCloseAbove)),
        (
            "init.lua",
            "-- TODO: x]]",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "init.lua",
            "-- TODO: x]==]",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("run", "# TODO: x'", Want::Skipped(Skip::MayCloseAbove)),
        (
            "a.c",
            "/* TODO: x\" + y */",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.vb", "' TODO: don't start", Want::End),
        (
            "a.py",
            "#!/usr/bin/env -S python3 -u # TODO: pick flags",
            Want::Skipped(Skip::NotWholeLine),
        ),
        ("a.jl", "# TODO: x =#", Want::Skipped(Skip::MayCloseAbove)),
        ("a.nim", "# TODO: x ]#", Want::Skipped(Skip::MayCloseAbove)),
        (
            "a.coffee",
            "### TODO: x",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "a.sql",
            "-- TODO: x $$;",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        // F4: Visual Basic's and batch's `REM` is not case-sensitive.
        ("a.bat", "rem TODO: pick a drive", Want::End),
        ("a.vb", "Rem TODO: pick a start", Want::End),
        // Review round 12, F1: a `'` may close a PHP, JavaScript or Lua string opened above.
        (
            "a.php",
            "// TODO: pick a cap';",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "a.php",
            "# TODO: pick a cap';",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "a.js",
            "// TODO: pick a cap';",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        (
            "init.lua",
            "-- TODO: x'",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        // F3: Postgres's `$tag$` dollar quote, and a Perl regex's own delimiter.
        (
            "a.sql",
            "-- TODO: x $body$;",
            Want::Skipped(Skip::MayCloseAbove),
        ),
        ("a.pl", "# TODO: x/x;", Want::Skipped(Skip::MayCloseAbove)),
    ];

    #[test]
    fn a_corpus_of_real_lines_is_skipped_or_gets_the_note_inside_its_whole_line_comment() {
        for &(file, line, want) in CORPUS {
            let out = code(file, line);

            match want {
                Want::End => assert_eq!(out, Ok(format!("{line} {NOTE}")), "{file}: {line}"),
                Want::BeforeCloser(closer) => {
                    let (head, _) = line.rsplit_once(closer).expect("the line has the closer");
                    let expected = format!("{} {NOTE} {closer}", head.trim_end());
                    assert_eq!(out, Ok(expected), "{file}: {line}");
                }
                Want::Skipped(skip) => assert_eq!(out, Err(skip), "{file}: {line}"),
            }
        }
    }
}
