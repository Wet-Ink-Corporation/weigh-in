//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the decision in, the rewritten line or the reason it was left alone out.
//!
//! A prose line (markdown, plain text) gets the note at its end. A code line gets it only when
//! a comment is the whole line, read in the file's language ([`Lang`], from its name):
//! - a line that starts with one of the language's line comment openers (not a `#!` shebang)
//!   gets it at the end, unless the line holds PHP's `?>`, which ends the comment, ends in a `\`
//!   line splice, or already holds a line break (a lone CR, U+0085, U+2028 or U+2029);
//! - a line that is exactly one block comment, `/* … */` or `<!-- … -->`, with nothing before
//!   its opener or after its closer, gets it just before the closer.
//!
//! Anything else is left as it was, a comment after code (`x = 1  # TODO`) included. In code the
//! option is written only when it holds nothing but letters, digits, spaces and
//! `. , : ; - _ + = / ! ?`, with `%` outside batch files and `'` where single-quoted strings stay
//! on one line, never three in a row ([`Skip::MayCloseString`] otherwise), and a line holding a
//! common closer of a string or block comment is left alone ([`Skip::MayCloseAbove`]).
//!
//! The reading is per line, so a whole-line comment may really sit inside a string or block
//! comment opened on an earlier line, and the note then changes that string. Those two refusals
//! narrow the risk but cannot close it: a byte string (`b"""…"""`, `br#"…"#`) cannot hold the
//! note's `→`, and a string with a delimiter of its own (Perl's `q!…!`, Ruby's `%q/…/`) may end
//! on any character. That residual risk is a known cost of the per-line rule; an ASCII note in
//! code would remove the byte-string case, and changing the marker is the owner's call.

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
        }
    }
}

/// The characters that could end a string, template or JSX text: quotes, backticks, `${`/`{`
/// interpolation braces and JSX's tag brackets (which cover `-->` and `?>` too).
const STRING_CLOSERS: [char; 6] = ['"', '`', '{', '}', '<', '>'];

/// The texts other than [`STRING_CLOSERS`] that close a string or block comment: `*/`, Lua's
/// `]]` and `]=`, Python's `'''`, Julia's `=#`, Nim's `]#`, CoffeeScript's `###` and
/// Postgres's `$$`.
const OTHER_CLOSERS: [&str; 8] = ["*/", "]]", "]=", "'''", "=#", "]#", "###", "$$"];

/// Whether `text`, a line of a file in `lang`, could close a string, template, JSX text or block
/// comment opened on an earlier line: [`STRING_CLOSERS`], [`OTHER_CLOSERS`], and `'` in the
/// languages whose single-quoted strings span lines (shell, SQL and files of unknown language).
fn may_close(text: &str, lang: Lang) -> bool {
    text.contains(STRING_CLOSERS)
        || OTHER_CLOSERS.iter().any(|c| text.contains(c))
        || (lang.single_quotes_span_lines() && text.contains('\''))
}

/// Whether `c` may be written into a comment of a file in `lang`: a letter, a digit, a space or
/// one of `. , : ; - _ + = / ! ?`; `%` except where batch files expand it; and `'` where
/// single-quoted strings stay on one line. Nothing else, so no sigil, bracket, quote or
/// backslash can interpolate into or close a string the comment may sit in.
fn safe_in_code(c: char, lang: Lang) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            ' ' | '.' | ',' | ':' | ';' | '-' | '_' | '+' | '=' | '/' | '!' | '?'
        )
        || (c == '%' && !lang.expands_percent())
        || (c == '\'' && !lang.single_quotes_span_lines())
}

/// Whether `option` may be written into a comment of a file in `lang`: every character is
/// [`safe_in_code`], and it holds no `'''`, which closes Groovy's and Dart's multi-line strings
/// even where a lone `'` is safe.
fn safe_option(option: &str, lang: Lang) -> bool {
    option.chars().all(|c| safe_in_code(c, lang)) && !option.contains("'''")
}

/// Whether `c` could end a line or a line comment: a control character (`\r` and U+0085
/// included), or the Unicode line and paragraph separators, which end a `//` comment in
/// JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
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
    if !lang.opens_line_comment(trimmed) {
        return Err(Skip::NotWholeLine);
    }
    if trimmed.contains("?>") {
        return Err(Skip::CodeAfterComment);
    }
    if trimmed.ends_with('\\') || line.chars().any(|c| c != '\t' && ends_a_line(c)) {
        return Err(Skip::RunsOn);
    }
    if may_close(trimmed, lang) {
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
    if decision.option.contains(block.option_forbids) {
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
    if may_close(inner, lang) {
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

        let out = write(Syntax::Code, Lang::Hash, "# TODO: pick", option);

        assert_eq!(
            out,
            Ok(format!(
                "# TODO: pick → Weigh-In wi-abc123: decided {option} (2026-10-07)"
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
    fn a_code_option_with_an_apostrophe_fits_where_single_quoted_strings_stay_on_one_line() {
        let note = "→ Weigh-In wi-abc123: decided don't cache (2026-10-07)";
        let read = |lang: Lang, line: &str| write(Syntax::Code, lang, line, "don't cache");

        assert_eq!(
            [
                read(Lang::CFamily, "// TODO: cache?"),
                read(Lang::Basic, "' TODO: cache?"),
                read(Lang::Hash, "# TODO: cache?"),
                read(Lang::Sql, "-- TODO: cache?"),
                read(Lang::Unknown, "# TODO: cache?"),
            ],
            [
                Ok(format!("// TODO: cache? {note}")),
                Ok(format!("' TODO: cache? {note}")),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
                Err(Skip::MayCloseString),
            ]
        );
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
        ("a.rs", "    // TODO: don't cache it", Want::End),
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
        ("run", "# TODO: a // b", Want::End),
        ("run", "    // TODO: see http://x", Want::End),
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
