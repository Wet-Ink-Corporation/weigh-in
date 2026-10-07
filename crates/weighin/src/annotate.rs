//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A prose line (markdown, plain text) gets the note at its end. A code line gets it only when
//! a comment is the whole line, read in the file's language ([`Lang`], from its name):
//! - a line that starts with one of the language's line comment openers gets it at the end,
//!   unless the line holds PHP's `?>`, which ends the comment, or ends in a `\` line splice;
//! - a line that is exactly one block comment, `/* … */` or `<!-- … -->`, with nothing before
//!   its opener or after its closer, gets it just before the closer.
//!
//! Anything else is left as it was, a comment after code (`x = 1  # TODO`) included, so a
//! decision never breaks the code around its question. The reading is per line: a line inside a
//! string or block comment opened on an earlier line is read as if it started in code.

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
    /// The note holds a line break (a control character, U+2028 or U+2029), a comment closer,
    /// PHP's `?>`, or text that would change the block comment it goes in (`/*`, or `--` in
    /// `<!-- -->`).
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
        }
    }
}

/// Whether `c` could end a line or a line comment: a control character, or the Unicode line
/// and paragraph separators, which end a `//` comment in JavaScript.
fn ends_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// `line` of a file read as `reading`, with `note` written into it, or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end the line or a comment; for a code line, the
/// [`Skip`] that says why its comment is not the whole line or could not hold the note.
pub(crate) fn annotate_source_line(
    line: &str,
    note: &str,
    reading: Reading,
) -> Result<String, Skip> {
    if note.chars().any(ends_a_line) || ["*/", "-->"].iter().any(|c| note.contains(c)) {
        return Err(Skip::UnsafeText);
    }
    match reading.syntax {
        Syntax::Prose => Ok(format!("{line} {note}")),
        Syntax::Code => annotate_code(line, note, reading.lang),
    }
}

/// [`annotate_source_line`] for a code line in `lang`.
fn annotate_code(line: &str, note: &str, lang: Lang) -> Result<String, Skip> {
    if note.contains("?>") {
        return Err(Skip::UnsafeText);
    }
    let trimmed = line.trim();
    if let Some(&block) = lang.blocks().iter().find(|b| trimmed.starts_with(b.open)) {
        return annotate_block(line, note, block);
    }
    if !lang.opens_line_comment(trimmed) {
        return Err(Skip::NotWholeLine);
    }
    if trimmed.contains("?>") {
        return Err(Skip::CodeAfterComment);
    }
    if trimmed.ends_with('\\') {
        return Err(Skip::RunsOn);
    }
    Ok(format!("{line} {note}"))
}

/// `line`, which starts (after indentation) with `block`'s opener, with `note` written just
/// before its closer, when that closer ends the line and the comment holds no other opener.
fn annotate_block(line: &str, note: &str, block: Block) -> Result<String, Skip> {
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
    fn a_note_that_would_change_its_block_comment_is_refused_there_only() {
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
            [Err(Skip::UnsafeText), Err(Skip::UnsafeText), Ok(())]
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
