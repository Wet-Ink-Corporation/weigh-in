//! The last check before a note is written into a comment that the line reading found. The
//! tokeniser is line-local and cannot know every language's literal syntax (a lifetime, a digit
//! separator, a triple quote, a regex literal, a verbatim string), so a quote it pairs wrongly can
//! hide that the comment marker sits inside a string. This check does not tokenise: it refuses a
//! comment whenever the raw text around its opener leaves room for such a mispairing, or the
//! comment ends before the line does (PHP's `?>`), or may be element text (JSX).

use super::lex::{Found, QUOTES};
use super::{Comment, Lang, Skip};

/// Whether the note may go in the comment `found` opens in `line`, read in `lang`: the comment
/// it ends up in under every reading starts there.
///
/// # Errors
/// [`Skip::CodeAfterComment`] for a line comment holding PHP's `?>`, which ends it;
/// [`Skip::InString`] when the quotes around the opener may be mispaired ([`mispaired`]);
/// [`Skip::Ambiguous`] when a regex literal before the opener holds a quote, or, in JSX, an
/// element tag precedes it, so the marker may be text.
pub(super) fn check(line: &str, found: Found, lang: Lang) -> Result<(), Skip> {
    let code = line.get(..found.start).unwrap_or(line);
    let text = line.get(found.body..).unwrap_or("");
    let php = matches!(lang, Lang::HashSlash | Lang::Unknown);
    if php && found.kind == Comment::Line && text.contains("?>") {
        return Err(Skip::CodeAfterComment);
    }
    if mispaired(code, text) {
        return Err(Skip::InString);
    }
    let regex = matches!(lang, Lang::CFamily | Lang::Jsx | Lang::Unknown);
    if (regex && regex_holds_quote(code)) || (lang == Lang::Jsx && holds_tag(code)) {
        return Err(Skip::Ambiguous);
    }
    Ok(())
}

/// Whether the quotes of `code`, the line before a comment opener, and `text`, the comment's
/// text, could be paired other than the tokeniser paired them: `code` holds a triple quote, a
/// quote character that `text` also holds (so a string opened before the opener may close after
/// it), or an odd number of `"` or backticks besides the character literals `'"'` and `` '`' ``
/// (so a string may run on past the line). A quote that a language does not use as one (an
/// apostrophe in the comment, a lifetime before it) can only make this refuse more lines.
fn mispaired(code: &str, text: &str) -> bool {
    let unpaired = |q: char| {
        let literal = format!("'{q}'");
        let count = code.matches(q).count();
        let in_literals = code.matches(literal.as_str()).count();
        !count.saturating_sub(in_literals).is_multiple_of(2)
    };
    code.contains("'''")
        || code.contains("\"\"\"")
        || QUOTES.iter().any(|&q| code.contains(q) && text.contains(q))
        || unpaired('"')
        || unpaired('`')
}

/// Whether `code` holds what may be a regex literal with a quote in it: a `/` at the line start
/// or after `(`, `,`, `=`, `:`, `[`, `!`, `&`, `|`, `?`, `{`, `}`, `;` or `return`, with a quote
/// before the next `/` or the end of the line.
fn regex_holds_quote(code: &str) -> bool {
    code.match_indices('/').any(|(at, slash)| {
        let before = code.get(..at).unwrap_or("").trim_end();
        let opens = before.is_empty()
            || before.ends_with(['(', ',', '=', ':', '[', '!', '&', '|', '?', '{', '}', ';'])
            || before.ends_with("return");
        opens
            && code
                .get(at.saturating_add(slash.len())..)
                .and_then(|rest| rest.split('/').next())
                .is_some_and(|body| body.contains(QUOTES))
    })
}

/// Whether `code` holds a JSX element tag: a `<` followed by a letter, `/` or `>`.
fn holds_tag(code: &str) -> bool {
    code.match_indices('<').any(|(at, lt)| {
        code.get(at.saturating_add(lt.len())..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '>'))
    })
}
