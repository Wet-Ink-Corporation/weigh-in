//! The source write-back of a decision: the line a question came from gets
//! `→ Weigh-In <id>: decided <option> (<date>)`, so it is not raised again. Pure: the line and
//! the note in, the rewritten line or the reason it was left alone out.
//!
//! A line is only ever changed inside its comment: the note goes at the end of a line comment, or
//! just before the closer of a block comment that ends the line. Anything else is left as it was,
//! so a decision never breaks the code around its question.

/// How the source file reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    /// Markdown: the line is prose, so the note goes at its end.
    Markdown,
    /// Anything else: the note goes only inside the line's comment.
    Code,
}

/// Why a source line was left as it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The note holds a line break, another control character or a comment closer.
    UnsafeText,
    /// Code follows the comment's closer on the line.
    CodeAfterComment,
    /// The line holds no comment the note could go in.
    NoComment,
}

impl Skip {
    /// The reason, as a receipt shows it.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::UnsafeText => "the option does not fit on one comment line",
            Self::CodeAfterComment => "code follows the comment",
            Self::NoComment => "no comment on the line",
        }
    }
}

/// The comment openers `weighin-core`'s TODO scan recognises, so every line it raised from is
/// read the same way here. A `*` counts only at the start of a line (a block comment's middle).
const OPENERS: &[&str] = &["//", "#", "--", "/*", "<!--", "*", "'", "REM "];

/// Block comment closers: text after one is code again.
const CLOSERS: &[&str] = &["*/", "-->"];

/// Where the line's first comment starts, and the length of its opener.
fn comment_start(line: &str) -> Option<(usize, usize)> {
    OPENERS
        .iter()
        .filter_map(|m| {
            line.find(m)
                .filter(|&p| m != &"*" || line.get(..p).is_some_and(|s| s.trim().is_empty()))
                .map(|p| (p, m.len()))
        })
        .min()
}

/// `line` with `note` written into its comment, or why it cannot be.
///
/// # Errors
/// [`Skip::UnsafeText`] when the note could end the comment or the line;
/// [`Skip::NoComment`] for a code line with no comment; [`Skip::CodeAfterComment`] when the
/// comment closes before the end of the line.
pub(crate) fn annotate_line(line: &str, note: &str, syntax: Syntax) -> Result<String, Skip> {
    if note.chars().any(char::is_control) || CLOSERS.iter().any(|c| note.contains(c)) {
        return Err(Skip::UnsafeText);
    }
    if syntax == Syntax::Markdown {
        return Ok(format!("{line} {note}"));
    }
    let (start, opener) = comment_start(line).ok_or(Skip::NoComment)?;
    let body_at = start.saturating_add(opener);
    let body = line.get(body_at..).ok_or(Skip::NoComment)?;
    let closer = CLOSERS
        .iter()
        .filter_map(|c| body.find(c).map(|p| (p, c.len())))
        .min();
    let Some((at, len)) = closer else {
        // A line comment, or a block comment that runs on: the line ends inside it.
        return Ok(format!("{line} {note}"));
    };
    let (before, closing) = line
        .split_at_checked(body_at.saturating_add(at))
        .ok_or(Skip::NoComment)?;
    let after = closing.get(len..).ok_or(Skip::NoComment)?;
    if !after.trim().is_empty() {
        return Err(Skip::CodeAfterComment);
    }
    Ok(format!("{} {note} {closing}", before.trim_end()))
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
    fn a_block_comment_that_runs_past_the_line_gets_the_note_at_the_end() {
        let out = annotate_line("/* TODO: pick a cap, because", NOTE, Syntax::Code);

        assert_eq!(out, Ok(format!("/* TODO: pick a cap, because {NOTE}")));
    }

    #[test]
    fn a_markdown_line_gets_the_note_at_the_end() {
        let out = annotate_line("- Should we shard by tenant?", NOTE, Syntax::Markdown);

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
            for syntax in [Syntax::Code, Syntax::Markdown] {
                let out = annotate_line("// TODO: pick a cap", note, syntax);

                assert_eq!(out, Err(Skip::UnsafeText), "{note:?}");
            }
        }
    }
}
