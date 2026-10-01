//! Lower-precision capture from what agents write to disk: bullets under "Open questions"-style
//! headings in documents, and newly written TODO/FIXME/HACK lines in code.
//!
//! Only the text an edit *wrote* is scanned (a `Write`'s content, an `Edit`'s new string), so an
//! existing TODO is not re-raised every time the file is touched.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub question: String,
    /// 1-based line within the scanned text.
    pub line: usize,
    pub heading: Option<String>,
    pub quote: String,
    pub todo: bool,
}

const HEADINGS: &[&str] = &[
    "open question",
    "open questions",
    "questions",
    "unresolved",
    "open issues",
    "assumptions",
    "assumption",
    "decisions needed",
    "decisions pending",
    "pending decisions",
    "needs decision",
    "needs human",
    "needs your input",
    "for review",
    "questions for",
    "follow-ups",
    "follow ups",
    "to decide",
];

fn heading_level(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&n) && t[n..].starts_with(' ') {
        let text = t[n..]
            .trim()
            .trim_matches(|c: char| c == '*' || c == '_' || c == ':')
            .trim()
            .to_owned();
        return Some((n, text));
    }
    // Bold pseudo-headings: **Open questions**
    if t.starts_with("**") && t.trim_end().ends_with("**") && t.len() > 4 {
        let text = t
            .trim()
            .trim_matches('*')
            .trim()
            .trim_end_matches(':')
            .to_owned();
        return Some((7, text));
    }
    None
}

fn is_question_heading(text: &str) -> bool {
    let lower = text.to_lowercase();
    let lower = lower.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    HEADINGS.iter().any(|h| {
        lower == *h
            || lower.starts_with(&format!("{h} "))
            || lower.starts_with(&format!("{h}:"))
            || lower.starts_with(&format!("{h} ("))
    })
}

fn bullet_text(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = if let Some(r) = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| t.strip_prefix("+ "))
    {
        r
    } else {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") ")) {
            &t[digits + 2..]
        } else {
            return None;
        }
    };
    // Checked boxes are resolved; unchecked or plain bullets are candidates.
    if rest.starts_with("[x]") || rest.starts_with("[X]") {
        return None;
    }
    let rest = rest.trim_start_matches("[ ]").trim();
    // Items already routed through Weigh-In carry an arrow annotation.
    if rest.contains("→ Weigh-In") || rest.is_empty() {
        return None;
    }
    Some(rest.to_owned())
}

/// Bullets under question-style headings in markdown. A bullet's wrapped continuation lines are
/// joined to it; nested bullets are detail and are skipped.
#[must_use]
pub fn scan_markdown(text: &str) -> Vec<Candidate> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut active: Option<(usize, String)> = None;
    let mut in_code = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if let Some((level, heading)) = heading_level(line) {
            if is_question_heading(&heading) {
                active = Some((level, heading));
            } else if active.as_ref().is_some_and(|(l, _)| level <= *l || *l == 7) {
                active = None;
            }
            continue;
        }
        let Some((_, heading)) = &active else {
            continue;
        };
        let indent = line.len() - line.trim_start().len();
        if indent > 1 {
            continue;
        }
        let Some(first) = bullet_text(line) else {
            continue;
        };
        let mut full = first;
        for next in lines.iter().skip(i + 1) {
            let nt = next.trim_start();
            let nindent = next.len() - nt.len();
            if nt.is_empty()
                || nindent < 2
                || bullet_text(next).is_some()
                || nt.starts_with('#')
                || nt.starts_with("```")
            {
                break;
            }
            full.push(' ');
            full.push_str(nt.trim_end());
        }
        let question = if full.starts_with("**") {
            full.trim_start_matches("**")
                .split("**")
                .next()
                .unwrap_or(&full)
                .trim()
                .to_owned()
        } else {
            full.clone()
        };
        let question = if question.len() < 8 {
            full.clone()
        } else {
            question
        };
        out.push(Candidate {
            question: clean(&question),
            line: i + 1,
            heading: Some(heading.clone()),
            quote: line.trim().to_owned(),
            todo: false,
        });
    }
    out
}

const TODO_TAGS: &[&str] = &["TODO", "FIXME", "HACK", "XXX"];

/// Newly written TODO/FIXME/HACK/XXX comments in code. `revisit later` style prose in comments is
/// caught too.
#[must_use]
pub fn scan_todos(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.contains("→ Weigh-In") {
            continue;
        }
        let Some(pos) = comment_start(line) else {
            continue;
        };
        let comment = &line[pos..];
        let upper = comment.to_ascii_uppercase();
        let tag_hit = TODO_TAGS.iter().find_map(|t| {
            upper.find(t).filter(|&p| {
                let before_ok = p == 0 || !upper.as_bytes()[p - 1].is_ascii_alphanumeric();
                let after = upper.as_bytes().get(p + t.len()).copied();
                before_ok && !after.is_some_and(|c| c.is_ascii_alphanumeric())
            })
        });
        let lower = comment.to_lowercase();
        let prose_hit = [
            "revisit later",
            "revisit this",
            "for now",
            "temporary",
            "temporarily",
            "hardcoded for now",
            "come back to",
        ]
        .iter()
        .any(|p| lower.contains(p));
        if tag_hit.is_none() && !prose_hit {
            continue;
        }
        let body = match tag_hit {
            Some(p) => {
                let after_tag = comment[p..].trim_start_matches(|c: char| c.is_ascii_alphabetic());
                // TODO(owner): ... carries an owner, not part of the question.
                let after_owner = match after_tag.strip_prefix('(') {
                    Some(rest) => rest.split_once(')').map_or(rest, |(_, r)| r),
                    None => after_tag,
                };
                after_owner.trim_start_matches([':', ' ', '-'])
            }
            None => comment,
        };
        let body = clean(body.trim_start_matches(['/', '#', '*', '-', ' ', '<', '!']));
        if body.len() < 4 {
            continue;
        }
        out.push(Candidate {
            question: body,
            line: i + 1,
            heading: None,
            quote: line.trim().to_owned(),
            todo: true,
        });
    }
    out
}

fn comment_start(line: &str) -> Option<usize> {
    ["//", "#", "--", "/*", "<!--", "*", "'", "REM "]
        .iter()
        .filter_map(|m| {
            line.find(m)
                .filter(|&p| m != &"*" || line[..p].trim().is_empty())
        })
        .min()
}

fn clean(s: &str) -> String {
    s.trim()
        .trim_end_matches("*/")
        .trim_end_matches("-->")
        .trim()
        .chars()
        .take(400)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullets_under_open_questions() {
        let doc = "# Plan\n\n## Steps\n- do a\n\n## Open questions\n- Should we shard by tenant?\n  - detail line\n- [x] Settled one\n- [ ] Which queue, Service Bus or Storage Queues?\n\n## Next\n- not a question\n";
        let c = scan_markdown(doc);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].question, "Should we shard by tenant?");
        assert_eq!(c[0].line, 7);
        assert_eq!(c[1].question, "Which queue, Service Bus or Storage Queues?");
    }

    #[test]
    fn wrapped_bullets_are_joined() {
        let doc = "## Open questions\n\n- **Does compliance require stronger isolation than an in-process\n  key** (e.g. separate caches)? This isolates by key only.\n- Rates change infrequently enough that a short staleness\n  window is acceptable.\n  - nested detail\n";
        let c = scan_markdown(doc);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(
            c[0].question,
            "Does compliance require stronger isolation than an in-process key"
        );
        assert_eq!(
            c[1].question,
            "Rates change infrequently enough that a short staleness window is acceptable."
        );
        assert_eq!(
            c[0].quote,
            "- **Does compliance require stronger isolation than an in-process"
        );
    }

    #[test]
    fn bold_heading_and_numbered_list() {
        let doc = "**Assumptions:**\n1. Tenants never share a key vault\n2) Retention is per series\n\nOther text";
        let c = scan_markdown(doc);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].question, "Retention is per series");
    }

    #[test]
    fn annotated_items_are_skipped() {
        let doc =
            "## Open questions\n- Use Postgres? → Weigh-In wi-3fa9c1: decided A (2026-09-26)\n";
        assert!(scan_markdown(doc).is_empty());
    }

    #[test]
    fn code_blocks_are_ignored() {
        let doc = "## Open questions\n```\n- not a bullet\n```\n- real one here\n";
        assert_eq!(scan_markdown(doc).len(), 1);
    }

    #[test]
    fn non_ascii_before_a_todo_does_not_panic_and_annotated_lines_are_skipped() {
        let c = scan_todos(
            "// ııé TODO: decide cache\n// TODO: pick one → Weigh-In wi-22d64d: decided yes (2026-09-27)\n",
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].question, "decide cache");
    }

    #[test]
    fn todos_in_code() {
        let code = "fn a() {\n    // TODO(ryan): decide whether to cache tokens per tenant\n    let x = 1; // hardcoded for now\n    let todos = 3; // counts todos\n}\n# FIXME: handle DST\n";
        let c = scan_todos(code);
        assert_eq!(c.len(), 3, "{c:?}");
        assert_eq!(c[0].question, "decide whether to cache tokens per tenant");
        assert!(c.iter().all(|x| x.todo));
    }
}
