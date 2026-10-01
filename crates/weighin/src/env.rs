//! Where the CLI is running: repository, common dir, worktree, branch, session and config.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use weighin_core::event::{EventEnvelope, LedgerEvent};

use crate::git;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardConfig {
    /// `artifact`, `local` or `off`.
    pub mode: String,
    pub url: Option<String>,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            mode: "artifact".into(),
            url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Caps {
    pub chat_cards: usize,
    pub board_focus: usize,
    /// Upper bound on a context pack, in characters (about 600 tokens).
    pub context_chars: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            chat_cards: 4,
            board_focus: 5,
            context_chars: 2400,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub markdown: bool,
    pub todos: bool,
    /// Repo-relative path prefixes never scanned.
    pub exclude: Vec<String>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            markdown: true,
            todos: true,
            exclude: vec![
                "target/".into(),
                "node_modules/".into(),
                ".git/".into(),
                ".kb/".into(),
                "dist/".into(),
                "vendor/".into(),
            ],
        }
    }
}

/// `.kb/_ledger/config.json`, committed with the repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `own` or `client`. Client repos default to a local board (decision wi-7c21e4).
    pub repo_class: String,
    pub owner: String,
    pub board: BoardConfig,
    pub caps: Caps,
    pub scan: ScanConfig,
    /// Where the ledger lives, when not the default. Relative paths resolve against the folder
    /// root; `~/` is the home directory. For a folder synced by OneDrive or Dropbox and used
    /// from more than one computer, point this outside the synced tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_class: "own".into(),
            owner: "ryan".into(),
            board: BoardConfig::default(),
            caps: Caps::default(),
            scan: ScanConfig::default(),
            store: None,
        }
    }
}

impl Config {
    pub const PATH: &'static str = ".kb/_ledger/config.json";

    #[must_use]
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join(Self::PATH))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// # Errors
    /// Fails when the directory or file cannot be written.
    pub fn save(&self, root: &Path) -> Result<PathBuf> {
        let p = root.join(Self::PATH);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(p)
    }
}

#[derive(Debug, Clone)]
pub struct Env {
    pub cwd: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub common_dir: Option<PathBuf>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub worktree: Option<String>,
    pub session: Option<String>,
    pub config: Config,
    /// `--store`, which wins over everything else.
    pub store_override: Option<PathBuf>,
}

/// Whether this process runs inside Cowork's cloud workspace, where the project directory is a
/// scratch folder deleted with the session and the user's files are on their computer.
#[must_use]
pub fn cowork() -> bool {
    std::env::var("CLAUDE_CODE_ENTRYPOINT").is_ok_and(|v| v == "remote_cowork")
}

/// `~/x` expanded with `HOME` (or `USERPROFILE` on Windows).
fn expand_home(p: &str) -> PathBuf {
    let rest = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\"));
    match rest {
        Some(rest) => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map_or_else(|| PathBuf::from(p), |h| PathBuf::from(h).join(rest)),
        None => PathBuf::from(p),
    }
}

impl Env {
    /// Discover the environment from `cwd`. Outside a git repository the ledger lives in
    /// `<cwd>/.weighin/` (Cowork folders, scratch directories).
    #[must_use]
    pub fn discover(cwd: &Path, session: Option<String>) -> Self {
        let repo_root = git::run(cwd, &["rev-parse", "--show-toplevel"])
            .ok()
            .map(|s| PathBuf::from(s.trim()));
        let common_dir = git::run(
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .ok()
        .map(|s| PathBuf::from(s.trim()));
        let branch = git::run(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let head = git::run(cwd, &["rev-parse", "--verify", "--quiet", "HEAD"])
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let worktree = repo_root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        // An empty value, or a `${…}` a surface did not substitute, is no session.
        let usable = |s: &String| !s.trim().is_empty() && !s.contains("${");
        let var = |k: &str| std::env::var(k).ok().filter(usable);
        let session = session
            .filter(usable)
            .or_else(|| var("WEIGHIN_SESSION"))
            .or_else(|| var("CLAUDE_SESSION_ID"))
            .or_else(|| var("CLAUDE_CODE_SESSION_ID"));
        let root = repo_root.clone().unwrap_or_else(|| cwd.to_path_buf());
        let config = Config::load(&root);
        Self {
            cwd: cwd.to_path_buf(),
            repo_root,
            common_dir,
            branch,
            head,
            worktree,
            session,
            config,
            store_override: None,
        }
    }

    /// Use `store` as the ledger, whatever the config and environment say.
    #[must_use]
    pub fn with_store(mut self, store: Option<PathBuf>) -> Self {
        self.store_override = store;
        self
    }

    /// Where `.kb` atoms and the config live: the current worktree's root.
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.repo_root.clone().unwrap_or_else(|| self.cwd.clone())
    }

    /// The live ledger, first match wins: `--store`; `WEIGHIN_STORE`; the config's `store`;
    /// `<git-common-dir>/weighin/ledger.sqlite3`, shared by every worktree and never committed
    /// (decision wi-6e2a47); outside git, `<folder>/.weighin/ledger.sqlite3`.
    #[must_use]
    pub fn store_path(&self) -> PathBuf {
        if let Some(p) = &self.store_override {
            return p.clone();
        }
        if let Ok(p) = std::env::var("WEIGHIN_STORE")
            && !p.trim().is_empty()
        {
            return expand_home(p.trim());
        }
        if let Some(p) = self.config.store.as_deref().map(str::trim)
            && !p.is_empty()
        {
            let p = expand_home(p);
            return if p.is_absolute() {
                p
            } else {
                self.root().join(p)
            };
        }
        match &self.common_dir {
            Some(c) => c.join("weighin").join("ledger.sqlite3"),
            None => self.cwd.join(".weighin").join("ledger.sqlite3"),
        }
    }

    /// The directory holding the ledger: board exports and the hook error log go beside it.
    #[must_use]
    pub fn state_dir(&self) -> PathBuf {
        self.store_path()
            .parent()
            .map_or_else(|| self.root().join(".weighin"), Path::to_path_buf)
    }

    #[must_use]
    pub fn envelope(&self, event: LedgerEvent, actor: &str) -> EventEnvelope {
        EventEnvelope {
            event,
            actor: actor.to_owned(),
            session_id: self.session.clone(),
            worktree: self.worktree.clone(),
            branch: self.branch.clone(),
            head: self.head.clone(),
            ts: now(),
        }
    }

    #[must_use]
    pub fn human(&self) -> String {
        format!("human:{}", self.config.owner)
    }

    /// Repo-relative, forward-slash form of a path the agent or a hook gave us.
    #[must_use]
    pub fn relative(&self, p: &str) -> String {
        let path = Path::new(p);
        let root = self.root();
        let rel = if path.is_absolute() {
            path.strip_prefix(&root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        };
        rel.to_string_lossy().replace('\\', "/")
    }

    /// # Errors
    /// Fails when the state directory cannot be created.
    pub fn ensure_state_dir(&self) -> Result<PathBuf> {
        let d = self.state_dir();
        std::fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;
        Ok(d)
    }
}

/// RFC 3339 UTC, seconds precision.
#[must_use]
pub fn now() -> String {
    let t = time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[must_use]
pub fn today() -> String {
    now().chars().take(10).collect()
}

/// Seconds between two RFC 3339 stamps, when both parse and the second is later.
#[must_use]
pub fn seconds_between(a: &str, b: &str) -> Option<u64> {
    let fmt = &time::format_description::well_known::Rfc3339;
    let a = time::OffsetDateTime::parse(a, fmt).ok()?;
    let b = time::OffsetDateTime::parse(b, fmt).ok()?;
    u64::try_from((b - a).whole_seconds()).ok()
}

/// Human age of a stamp relative to now: `12m`, `3h`, `2d`.
#[must_use]
pub fn age(ts: &str) -> String {
    match seconds_between(ts, &now()) {
        Some(s) if s < 3600 => format!("{}m", s / 60),
        Some(s) if s < 172_800 => format!("{}h", s / 3600),
        Some(s) => format!("{}d", s / 86_400),
        None => "?".into(),
    }
}
