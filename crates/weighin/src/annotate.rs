//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A prose line (markdown, plain text) gets the note at its end. A code line gets it only when
//! a comment is the whole line, read in the file's language ([`Lang`], from its name):
//! - a line that starts with one of the language's line comment openers gets it at the end,
//!   unless the line holds PHP's `?>`, which ends the comment, ends in a `\` line splice, or
//!   already holds a line break (a lone CR, U+0085, U+2028 or U+2029) that ends the comment;
//! - a line that is exactly one block comment, `/* … */` or `<!-- … -->`, with nothing before
//!   its opener or after its closer, gets it just before the closer.
//!
//! Anything else is left as it was, a comment after code (`x = 1  # TODO`) included, so a
//! decision never breaks the code around its question. The reading is per line, so a whole-line
//! comment may really sit inside a string, template or block comment opened on an earlier line.
//! Two refusals keep that case safe, at the cost of skipping some harmless lines:
//! - a line holding text that could close such a construct (a quote, a backtick, a brace, an
//!   angle bracket, `*/`, `]]` or `]=`, and `'` where single-quoted strings span lines) is left
//!   alone ([`Skip::MayCloseAbove`]), as the note would land after the closer, in code;
//! - a code note holding such text, or a backslash, which is an escape inside a string, is
//!   refused ([`Skip::MayCloseString`]).

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

/// Why a source line was left as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The note holds a line break (a control character, U+2028 or U+2029), or, in code, a
    /// comment opener or closer (which nests or ends a block comment around the line), PHP's
    /// `?>`, or `--` in `<!-- -->`.
    UnsafeText,
    /// The line is not a comment of the file's language from its start: code, a string or a
    /// block comment's middle line may hold the marker.
    NotWholeLine,
    /// Code follows the comment on the line: after a block comment's closer, or after PHP's `?>`.
    CodeAfterComment,
    /// The comment runs on to the next line: a line comment ending in a `\` line splice, or a
    /// block comment that does not close on the line.
    RunsOn,
    /// The block comment holds another opener, which nests in some languages.
    Nested,
    /// The note holds text that could end or change a string, template or JSX text opened on an
    /// earlier line, should the comment sit inside one: see [`may_close`], and a backslash.
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
                "the option holds a quote, bracket or backslash that could end or change a string around the comment"
            }
            Self::MayCloseAbove => "the line may close a string or comment opened above",
        }
    }
}

/// The characters that could end a string, template or JSX text: quotes, backticks, `${`/`{`
/// interpolation braces and JSX's tag brackets (which cover `-->` and `?>` too).
const STRING_CLOSERS: [char; 6] = ['"', '`', '{', '}', '<', '>'];

/// Whether `text`, in a file of `lang`, could close a string, template, JSX text or block
/// comment opened on an earlier line: [`STRING_CLOSERS`], `*/`, Lua's `]]` and `]=`, Python's
/// `'''`, and `'` in the languages whose single-quoted strings span lines (shell, SQL and
/// files of unknown language).
fn may_close(text: &str, lang: Lang) -> bool {
    text.contains(STRING_CLOSERS)
        || ["*/", "]]", "]=", "'''"].iter().any(|c| text.contains(c))
        || (lang.single_quotes_span_lines() && text.contains('\''))
}

/// Whether `c` could end a line or a line comment: a control character (`\r` and U+0085
/// included), or the Unicode line and paragraph separators, which end a `//` comment in
/// JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// `line` of a file read as `reading`, with `note` written into it, or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end the line (or, in code, open or end a comment);
/// [`Skip::MayCloseString`] when a code note could end or change a string around the comment;
/// for a code line, the [`Skip`] that says why its comment is not the whole line or could not
/// hold the note.
pub(crate) fn annotate_source_line(
    line: &str,
    note: &str,
    reading: Reading,
) -> Result<String, Skip> {
    if note.chars().any(ends_a_line) {
        return Err(Skip::UnsafeText);
    }
    match reading.syntax {
        Syntax::Prose => Ok(format!("{line} {note}")),
        Syntax::Code => annotate_code(line, note, reading.lang),
    }
}

/// [`annotate_source_line`] for a code line in `lang`.
fn annotate_code(line: &str, note: &str, lang: Lang) -> Result<String, Skip> {
    if ["*/", "-->", "?>", "/*", "<!--"]
        .iter()
        .any(|c| note.contains(c))
    {
        return Err(Skip::UnsafeText);
    }
    if note.contains('\\') || may_close(note, lang) {
        return Err(Skip::MayCloseString);
    }
    let trimmed = line.trim();
    if let Some(&block) = lang.blocks().iter().find(|b| trimmed.starts_with(b.open)) {
        return annotate_block(line, note, block, lang);
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
    Ok(format!("{line} {note}"))
}

/// `line`, which starts (after indentation) with `block`'s opener, with `note` written just
/// before its closer, when that closer ends the line, the comment holds no other opener and
/// nothing in it could close a construct opened above.
fn annotate_block(line: &str, note: &str, block: Block, lang: Lang) -> Result<String, Skip> {
    if note.contains(block.note_forbids) {
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
    Ok(format!("{} {note} {closing}", head.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "→ Weigh-In wi-abc123: decided 4096 (2026-10-07)";

    /// `line` of the file `file`, read as code, with [`NOTE`] written into it.
    fn code(file: &str, line: &str) -> Result<String, Skip> {
        let reading = Reading {
            syntax: Syntax::Code,
            lang: Lang::of(file),
        };
        annotate_source_line(line, NOTE, reading)
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
        let reading = Reading {
            syntax: Syntax::Prose,
            lang: Lang::Unknown,
        };

        let out = annotate_source_line("- Should we shard by tenant?", NOTE, reading);

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
    fn a_note_that_would_break_the_line_is_refused() {
        for option in [
            "10\nreturn 99;",
            "a\u{2028}b",
            "x */ y",
            "x --> y",
            "x ?> y",
            "a\tb",
        ] {
            let note = format!("→ Weigh-In wi-abc123: decided {option} (2026-10-07)");
            let reading = Reading {
                syntax: Syntax::Code,
                lang: Lang::CFamily,
            };

            let out = annotate_source_line("// TODO: pick a cap", &note, reading);

            assert_eq!(out, Err(Skip::UnsafeText), "{option:?}");
        }
    }

    #[test]
    fn a_prose_note_with_a_line_break_is_refused() {
        let reading = Reading {
            syntax: Syntax::Prose,
            lang: Lang::Unknown,
        };

        let out = annotate_source_line("- Shard?", "decided a\nb", reading);

        assert_eq!(out, Err(Skip::UnsafeText));
    }

    #[test]
    fn a_note_that_would_open_or_change_a_block_comment_is_refused() {
        let star = "→ Weigh-In wi-abc123: decided a /* b (2026-10-07)";
        let dashes = "→ Weigh-In wi-abc123: decided a -- b (2026-10-07)";
        let read = |file: &str, line: &str, note: &str| {
            let reading = Reading {
                syntax: Syntax::Code,
                lang: Lang::of(file),
            };
            annotate_source_line(line, note, reading)
        };

        assert_eq!(
            [
                read("a.c", "/* TODO: pick */", star),
                read("a.html", "<!-- TODO: pick -->", dashes),
                read("a.c", "// TODO: pick", star),
            ]
            .map(|r| r.map(|_| ())),
            [Err(Skip::UnsafeText); 3]
        );
    }

    #[test]
    fn a_code_note_that_could_close_a_string_around_its_comment_is_refused() {
        for option in [
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
            r"C:\q",
        ] {
            let note = format!("→ Weigh-In wi-abc123: decided {option} (2026-10-07)");
            let reading = Reading {
                syntax: Syntax::Code,
                lang: Lang::CFamily,
            };

            let out = annotate_source_line("// TODO: pick a cap", &note, reading);

            assert_eq!(out, Err(Skip::MayCloseString), "{option:?}");
        }
    }

    #[test]
    fn a_code_note_with_an_apostrophe_fits_where_single_quoted_strings_stay_on_one_line() {
        let note = "→ Weigh-In wi-abc123: decided don't cache (2026-10-07)";
        let read = |lang: Lang, line: &str| {
            let reading = Reading {
                syntax: Syntax::Code,
                lang,
            };
            annotate_source_line(line, note, reading)
        };

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
    fn a_prose_note_may_hold_comment_closers_and_quotes() {
        let note = r#"→ Weigh-In wi-abc123: decided "a */ b --> c ?>" <d> (2026-10-07)"#;
        let reading = Reading {
            syntax: Syntax::Prose,
            lang: Lang::Unknown,
        };

        let out = annotate_source_line("- Shard?", note, reading);

        assert_eq!(out, Ok(format!("- Shard? {note}")));
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
