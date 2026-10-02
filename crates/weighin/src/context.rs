//! Context packs: the evidence a card needs, computed by script so the model reads a short
//! summary instead of files. Bounded by `caps.context_chars` (about 600 tokens).
//!
//! For a `path:line` anchor: a 3–15 line excerpt, the symbol's reference count and top call
//! sites, tests that mention it, one line of history, working-tree churn, matching `.kb`
//! atoms, a SHA-pinned permalink and an editor link. Without an anchor, only atom matches.

use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};
use weighin_core::Item;

use crate::env::{self, Env};
use crate::git;

#[derive(Debug, Default, Serialize)]
pub struct Pack {
    pub item: String,
    pub anchor: Option<String>,
    pub excerpt: Option<(usize, Vec<String>)>,
    pub symbol: Option<String>,
    pub references: Option<usize>,
    pub call_sites: Vec<String>,
    pub tests: Vec<String>,
    pub history: Option<String>,
    pub churn: Option<String>,
    pub atoms: Vec<String>,
    pub permalink: Option<String>,
    pub editor: Option<String>,
    pub notes: Vec<String>,
}

fn parse_anchor(at: &str) -> (&str, Option<usize>) {
    // path:line or path:line:col; Windows drive letters keep their colon.
    let (path, position) = env::split_anchor(at);
    let line = position
        .strip_prefix(':')
        .and_then(|p| p.split(':').next())
        .and_then(|l| l.parse().ok());
    (path, line)
}

const DEF_WORDS: &[&str] = &[
    "fn",
    "func",
    "function",
    "def",
    "class",
    "struct",
    "enum",
    "trait",
    "interface",
    "type",
    "record",
    "impl",
    "module",
    "namespace",
    "const",
    "static",
    "sub",
    "procedure",
];

/// The identifier an anchor line defines or most prominently uses.
fn symbol_on(line: &str) -> Option<String> {
    let words: Vec<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        if DEF_WORDS.contains(&w.to_ascii_lowercase().as_str())
            && let Some(n) = words.get(i + 1)
            && n.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            return Some((*n).to_owned());
        }
    }
    words
        .iter()
        .filter(|w| {
            w.len() >= 4
                && w.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
        })
        .max_by_key(|w| w.len())
        .map(|w| (*w).to_owned())
}

fn has(cmd: &str) -> bool {
    Command::new(cmd)
        .stdin(Stdio::null())
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// `path:line` hits for a whole-word symbol, excluding generated trees.
fn references(root: &Path, sym: &str) -> Vec<String> {
    // An explicit `.` and a null stdin: ripgrep searches stdin, not the tree, whenever stdin is
    // not a terminal, which is always the case inside hooks and tool calls.
    let out = if has("rg") {
        Command::new("rg")
            .current_dir(root)
            .stdin(Stdio::null())
            .args([
                "-n",
                "-w",
                "--no-heading",
                "--color=never",
                "-g",
                "!target",
                "-g",
                "!node_modules",
                "-g",
                "!.git",
                "-g",
                "!.kb/_intake",
                "-F",
                "--",
                sym,
                ".",
            ])
            .output()
            .ok()
    } else {
        Command::new("git")
            .current_dir(root)
            .stdin(Stdio::null())
            .args(["grep", "-n", "-w", "-F", "--", sym])
            .output()
            .ok()
    };
    let Some(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim_start_matches("./").replace('\\', "/"))
        .collect()
}

fn is_test_path(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.contains("/test")
        || l.contains("tests/")
        || l.contains("_test.")
        || l.contains(".test.")
        || l.contains("spec")
        || l.starts_with("test")
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Atom ids and titles in `.kb` (outside `_intake` and `_ledger`) that mention any needle.
fn matching_atoms(root: &Path, needles: &[&str]) -> Vec<String> {
    let kb = root.join(".kb");
    if !kb.is_dir() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut stack = vec![kb];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !name.starts_with('_') {
                    stack.push(p);
                }
                continue;
            }
            if !name.ends_with(".md") || name.eq_ignore_ascii_case("readme.md") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            if needles.iter().any(|n| !n.is_empty() && text.contains(n)) {
                let field = |k: &str| {
                    text.lines().find_map(|l| {
                        l.strip_prefix(k)
                            .map(|v| v.trim().trim_matches('"').to_owned())
                    })
                };
                let id = field("id:").unwrap_or(name.clone());
                let status = field("status:").unwrap_or_default();
                let title = field("title:").unwrap_or_default();
                hits.push(format!("{id} [{status}] {}", clip(&title, 70)));
            }
        }
    }
    hits.sort();
    hits.truncate(4);
    hits
}

/// Build the pack for an item.
#[must_use]
pub fn build(env: &Env, item: &Item) -> Pack {
    let root = env.root();
    let mut p = Pack {
        item: item.id.to_string(),
        ..Default::default()
    };
    let Some(at) = item.at.clone().or_else(|| {
        item.source_path
            .as_ref()
            .map(|s| format!("{s}:{}", item.source_line.unwrap_or(1)))
    }) else {
        p.atoms = matching_atoms(&root, &[&item.question]);
        p.notes
            .push("no path:line anchor; evidence limited to .kb matches".into());
        return p;
    };
    let (path, line) = parse_anchor(&at);
    let rel = env.relative(path);
    p.anchor = Some(format!("{rel}:{}", line.unwrap_or(1)));
    let abs = root.join(&rel);
    let text = std::fs::read_to_string(&abs).ok();
    if text.is_none() {
        p.notes.push(format!("{rel} not found in this worktree"));
    }
    let line = line.unwrap_or(1).max(1);
    let mut anchor_line = String::new();
    if let Some(t) = &text {
        let lines: Vec<&str> = t.lines().collect();
        let start = line.saturating_sub(3).max(1);
        let end = (line + 7).min(lines.len()).max(start);
        let body: Vec<String> = lines
            .get(start - 1..end)
            .unwrap_or_default()
            .iter()
            .map(|l| clip(l, 140))
            .collect();
        anchor_line = lines.get(line - 1).copied().unwrap_or_default().to_owned();
        p.excerpt = Some((start, body));
    }
    p.symbol = item.sym.clone().or_else(|| symbol_on(&anchor_line));
    if let Some(sym) = &p.symbol {
        let refs: Vec<String> = references(&root, sym)
            .into_iter()
            .filter(|r| !r.starts_with(&format!("{rel}:{line}:")))
            .collect();
        p.references = Some(refs.len());
        p.tests = refs
            .iter()
            .filter(|r| is_test_path(r))
            .take(3)
            .map(|r| clip(r, 120))
            .collect();
        p.call_sites = refs
            .iter()
            .filter(|r| !is_test_path(r))
            .take(5)
            .map(|r| clip(r, 120))
            .collect();
    }
    let range = format!("-L{line},{}:{rel}", line + 5);
    if let Ok(log) = git::run(
        &root,
        &[
            "log",
            "--no-patch",
            "--format=%h %ad %s",
            "--date=short",
            &range,
        ],
    ) {
        let commits: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).collect();
        if let Some(last) = commits.first() {
            p.history = Some(format!(
                "{} commit(s) touch these lines; last {}",
                commits.len(),
                clip(last, 90)
            ));
        }
    }
    if let Ok(ns) = git::run(&root, &["diff", "--numstat", "HEAD", "--", &rel])
        && let Some(l) = ns.lines().next()
    {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() >= 2 {
            p.churn = Some(format!("uncommitted +{} -{}", f[0], f[1]));
        }
    }
    let sym = p.symbol.clone().unwrap_or_default();
    p.atoms = matching_atoms(&root, &[&rel, &sym]);
    if let Some(sha) = &env.head {
        let (a, b) = p
            .excerpt
            .as_ref()
            .map_or((line, line), |(s, l)| (*s, s + l.len().saturating_sub(1)));
        p.permalink = git::permalink(&root, sha, &rel, a, b);
    }
    let abs_s = abs.to_string_lossy().replace('\\', "/");
    // vscode://file/<absolute path>: `/home/x` becomes `vscode://file/home/x`, `C:/r/a` becomes
    // `vscode://file/C:/r/a`.
    p.editor = Some(format!(
        "vscode://file/{}:{line}:1",
        abs_s.trim_start_matches('/')
    ));
    p
}

/// Compact text within `cap` characters.
#[must_use]
pub fn render(p: &Pack, cap: usize) -> String {
    let mut s = format!("## {} context", p.item);
    if let Some(a) = &p.anchor {
        s.push_str(&format!(" · {a}"));
    }
    s.push('\n');
    if let Some((start, lines)) = &p.excerpt {
        s.push_str("```\n");
        for (i, l) in lines.iter().enumerate() {
            s.push_str(&format!("{:>4} {l}\n", start + i));
        }
        s.push_str("```\n");
    }
    if let Some(sym) = &p.symbol {
        s.push_str(&format!(
            "symbol `{sym}`: {} other reference(s)\n",
            p.references.unwrap_or(0)
        ));
    }
    if !p.call_sites.is_empty() {
        s.push_str(&format!("call sites: {}\n", p.call_sites.join(" | ")));
    }
    if !p.tests.is_empty() {
        s.push_str(&format!("tests: {}\n", p.tests.join(" | ")));
    }
    for (k, v) in [
        ("history", &p.history),
        ("churn", &p.churn),
        ("permalink", &p.permalink),
        ("editor", &p.editor),
    ] {
        if let Some(v) = v {
            s.push_str(&format!("{k}: {v}\n"));
        }
    }
    if !p.atoms.is_empty() {
        s.push_str(&format!(".kb: {}\n", p.atoms.join(" | ")));
    }
    for n in &p.notes {
        s.push_str(&format!("note: {n}\n"));
    }
    if s.chars().count() > cap {
        s = s.chars().take(cap.saturating_sub(12)).collect::<String>() + "\n[truncated]";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_parse() {
        assert_eq!(parse_anchor("src/a.rs:88"), ("src/a.rs", Some(88)));
        assert_eq!(parse_anchor("src/a.rs:88:4"), ("src/a.rs", Some(88)));
        assert_eq!(parse_anchor("C:/r/a.cs:12"), ("C:/r/a.cs", Some(12)));
        assert_eq!(parse_anchor("docs/plan.md"), ("docs/plan.md", None));
    }

    #[test]
    fn symbols() {
        assert_eq!(
            symbol_on("pub fn store_dir(common_dir: &Path) -> PathBuf {").as_deref(),
            Some("store_dir")
        );
        assert_eq!(
            symbol_on("public sealed class RetentionPolicy : IPolicy").as_deref(),
            Some("RetentionPolicy")
        );
        assert_eq!(
            symbol_on("    let overrides = policy.series_overrides();").as_deref(),
            Some("series_overrides")
        );
    }
}
