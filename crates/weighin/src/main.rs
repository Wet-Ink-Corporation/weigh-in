//! `weighin`: the single writer of the Weigh-In decision ledger.
//!
//! Output is written for a model to read at the lowest token cost: short lines, no decoration,
//! `--json` where a structure is needed.

#![forbid(unsafe_code)]

mod context;
mod env;
mod git;
mod hooks;
mod ops;
mod store;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use std::io::Read as _;
use std::path::PathBuf;
use weighin_core::board::{self, Answer, Scope, ValidAnswer};
use weighin_core::marker::{Door, parse_markers};
use weighin_core::projection::Health;
use weighin_core::{Item, Status};

use crate::env::{Config, Env};
use crate::ops::{Op, RaiseInput, Session};

#[derive(Parser)]
#[command(name = "weighin", version, about = "Weigh-In decision ledger")]
struct Cli {
    /// Session key (defaults to $WEIGHIN_SESSION, $CLAUDE_SESSION_ID, $CLAUDE_CODE_SESSION_ID).
    #[arg(long, global = true)]
    session: Option<String>,
    /// Work as if run from this directory.
    #[arg(long, global = true)]
    dir: Option<PathBuf>,
    /// Use this ledger file (wins over $WEIGHIN_STORE and the config's `store`).
    #[arg(long, global = true)]
    store: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Hook entry point: reads the hook JSON on stdin.
    Hook,
    /// Print the marker convention and open counts.
    Prime,
    /// Create .kb/_ledger/config.json.
    Init {
        /// `own` or `client` (client repos default to a local board).
        #[arg(long, default_value = "own")]
        class: String,
        /// `artifact`, `local` or `off`.
        #[arg(long)]
        board: Option<String>,
        #[arg(long)]
        board_url: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        /// Keep the ledger here instead of the default (relative to the folder, or absolute).
        #[arg(long = "ledger")]
        ledger: Option<String>,
    },
    /// List open items (this session by default when a session is known).
    Open {
        #[arg(long)]
        repo: bool,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 12)]
        top: usize,
        /// Append a context pack for each listed item that has a path anchor.
        #[arg(long)]
        with_context: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show one item in full.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Script-built evidence for one item (about 600 tokens at most).
    Context {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Raise a question by hand, or every `weigh-in` block on stdin with --markers.
    Raise {
        #[arg(long)]
        q: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        door: Option<String>,
        #[arg(long)]
        authority: Option<String>,
        #[arg(long)]
        did: Option<String>,
        #[arg(long)]
        flip: Option<String>,
        #[arg(long)]
        at: Option<String>,
        /// Read `weigh-in` blocks from stdin (the in-sitting sweep).
        #[arg(long)]
        markers: bool,
        #[arg(long, default_value = "manual")]
        channel: String,
    },
    /// Apply a JSON array of operations from stdin (or --file) in one call.
    Batch {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Record a decision.
    Decide {
        id: String,
        option: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long, default_value = "cli")]
        via: String,
    },
    /// Defer with a named fact, a trigger, a resolve-by and a default.
    Defer {
        id: String,
        #[arg(long)]
        flip: String,
        #[arg(long)]
        trigger: String,
        #[arg(long = "by")]
        resolve_by: String,
        #[arg(long)]
        default: String,
        #[arg(long)]
        reason: Option<String>,
    },
    /// Dismiss: `not-useful`, `recoverable`, `moot`, `already-decided:<ref>`, `duplicate-of:<id>`, `not-my-call:<who>`.
    Dismiss {
        id: String,
        why: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reopen a decided, defaulted or dismissed item.
    Reopen {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Defaults taken without you, and unattended items not yet triaged.
    Digest {
        #[arg(long)]
        json: bool,
    },
    /// Action rate, not-useful rate, overrides, time to decide.
    Stats {
        #[arg(long)]
        json: bool,
    },
    /// The commit trailer for a decided item.
    Trailer { id: String },
    /// Raise every open question and new TODO in a document or source file (backfill).
    Scan { paths: Vec<PathBuf> },
    /// Exchange with the Artifact board.
    Board {
        #[command(subcommand)]
        cmd: BoardCmd,
    },
}

#[derive(Subcommand)]
enum BoardCmd {
    /// Write the board state document (push it with ArtifactData set, file_path).
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Apply answers read back from the board (a JSON file, an array, or a directory of documents).
    Apply { path: PathBuf },
}

fn main() {
    let cli = Cli::parse();
    if matches!(cli.cmd, Cmd::Hook) {
        let mut input = String::new();
        let _ = std::io::stdin().read_to_string(&mut input);
        // A hook never fails a session, not even on a bug: a panic prints nothing and exits 0.
        std::panic::set_hook(Box::new(|_| {}));
        if let Ok(out) = std::panic::catch_unwind(|| hooks::run(&input)) {
            print!("{out}");
        }
        return;
    }
    if let Err(e) = run(cli) {
        eprintln!("weighin: {e:#}");
        std::process::exit(1);
    }
}

fn stdin() -> Result<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn run(cli: Cli) -> Result<()> {
    let cwd = match cli.dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let env = Env::discover(&cwd, cli.session.clone()).with_store(cli.store.clone());
    match cli.cmd {
        Cmd::Hook => unreachable!(),
        Cmd::Prime => print!("{}", hooks::prime(&env, "manual")?),
        Cmd::Init {
            class,
            board,
            board_url,
            owner,
            ledger,
        } => {
            let mut c = Config::load(&env.root());
            c.repo_class = class.clone();
            c.board.mode = board.unwrap_or_else(|| {
                if class == "client" {
                    "local".into()
                } else {
                    "artifact".into()
                }
            });
            if board_url.is_some() {
                c.board.url = board_url;
            }
            if let Some(o) = owner {
                c.owner = o;
            }
            if let Some(l) = ledger {
                c.store = Some(l).filter(|l| !l.trim().is_empty());
            }
            let p = c.save(&env.root())?;
            let env = Env {
                config: c.clone(),
                ..env
            };
            println!(
                "wrote {} · class {} · board {} · ledger {}",
                env.relative(&p.to_string_lossy()),
                c.repo_class,
                c.board.mode,
                env.store_path().display()
            );
        }
        Cmd::Open {
            repo,
            all,
            top,
            with_context,
            json,
        } => {
            let s = Session::open(&env, "cli")?;
            let scope_session = if repo || all {
                None
            } else {
                env.session.clone()
            };
            let items: Vec<&Item> = if all {
                s.ledger.items.values().collect()
            } else {
                match &scope_session {
                    Some(id) => s.ledger.open_in_session(id),
                    None => s.ledger.open(),
                }
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&items.iter().take(top).collect::<Vec<_>>())?
                );
                return Ok(());
            }
            let repo_open = s.ledger.open();
            let one_way = repo_open
                .iter()
                .filter(|i| i.door() == Door::OneWay)
                .count();
            let defaults = s.ledger.unacknowledged_defaults().len();
            let here = scope_session.as_deref().map_or_else(String::new, |id| {
                format!(
                    "{} open in this session · ",
                    s.ledger.open_in_session(id).len()
                )
            });
            println!(
                "{here}{} open in repo ({one_way} one-way) · {defaults} default(s) to review",
                repo_open.len()
            );
            if items.is_empty() {
                println!("No decisions need you.");
            }
            for i in items.iter().take(top) {
                print!("{}", line(i));
            }
            if items.len() > top {
                println!("… {} more (--top)", items.len() - top);
            }
            if with_context {
                for i in items
                    .iter()
                    .take(top)
                    .filter(|i| i.needs_attention() && i.at.is_some())
                {
                    println!(
                        "{}",
                        context::render(&context::build(&env, i), env.config.caps.context_chars)
                    );
                }
            }
        }
        Cmd::Show { id, json } => {
            let s = Session::open(&env, "cli")?;
            let item = s
                .ledger
                .resolve(&id)
                .ok_or_else(|| anyhow!("no single item matches `{id}`"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(item)?);
            } else {
                print!("{}", line(item));
                if let Some(c) = &item.card {
                    println!("  card: {}", serde_json::to_string(c)?);
                }
                if let Some(d) = &item.decision {
                    println!(
                        "  decided {} by {} ({}) via {} · {}",
                        d.option,
                        d.decider,
                        d.provenance.as_str(),
                        d.via,
                        d.ts
                    );
                }
                for r in &item.records {
                    println!("  record: {} ({})", r.path, r.atom_id);
                }
            }
        }
        Cmd::Context { id, json } => {
            let s = Session::open(&env, "cli")?;
            let item = s
                .ledger
                .resolve(&id)
                .ok_or_else(|| anyhow!("no single item matches `{id}`"))?;
            let pack = context::build(&env, item);
            if json {
                println!("{}", serde_json::to_string(&pack)?);
            } else {
                println!("{}", context::render(&pack, env.config.caps.context_chars));
            }
        }
        Cmd::Raise {
            q,
            kind,
            door,
            authority,
            did,
            flip,
            at,
            markers,
            channel,
        } => {
            let mut s = Session::open(&env, "cli")?;
            let inputs: Vec<RaiseInput> = if markers {
                let text = stdin()?;
                let ch = if channel == "manual" {
                    "sweep".to_owned()
                } else {
                    channel
                };
                parse_markers(&text)
                    .iter()
                    .map(|m| {
                        let mut r = RaiseInput::from_marker(m, weighin_core::event::Channel::Sweep);
                        r.channel = Some(ch.clone());
                        r
                    })
                    .collect()
            } else {
                let q = q.ok_or_else(|| {
                    anyhow!("--q is required (or --markers with blocks on stdin)")
                })?;
                vec![RaiseInput {
                    q,
                    kind,
                    door,
                    authority,
                    did,
                    flip,
                    at,
                    channel: Some(channel),
                    ..Default::default()
                }]
            };
            if inputs.is_empty() {
                bail!("no weigh-in blocks found on stdin");
            }
            for r in &inputs {
                println!("{}", s.apply(&Op::Raise(r.clone()))?);
            }
        }
        Cmd::Batch { file } => {
            let text = match file {
                Some(f) => std::fs::read_to_string(&f)
                    .with_context(|| format!("reading {}", f.display()))?,
                None => stdin()?,
            };
            let ops = ops::parse_ops(&text)?;
            let mut s = Session::open(&env, "cli")?;
            let mut failed = 0;
            for (n, op) in ops.iter().enumerate() {
                let op = match op {
                    Ok(op) => op,
                    Err(e) => {
                        failed += 1;
                        println!("✗ op {n}: {e:#}");
                        continue;
                    }
                };
                match s.apply(op) {
                    Ok(r) => println!("✓ {r}"),
                    Err(e) => {
                        failed += 1;
                        println!("✗ op {n}: {e:#}");
                    }
                }
            }
            if failed > 0 {
                bail!(
                    "{failed} of {} operations failed; the rest were applied",
                    ops.len()
                );
            }
        }
        Cmd::Decide {
            id,
            option,
            note,
            via,
        } => {
            let mut s = Session::open(&env, "cli")?;
            println!(
                "{}",
                s.apply(&Op::Decide {
                    id,
                    option,
                    note,
                    provenance: None,
                    via: Some(via),
                    seconds: None
                })?
            );
        }
        Cmd::Defer {
            id,
            flip,
            trigger,
            resolve_by,
            default,
            reason,
        } => {
            let mut s = Session::open(&env, "cli")?;
            println!(
                "{}",
                s.apply(&Op::Defer {
                    id,
                    flip,
                    trigger,
                    resolve_by,
                    default,
                    owner: None,
                    unqualified: false,
                    reason: reason.unwrap_or_else(|| "deferred with a named fact".into())
                })?
            );
        }
        Cmd::Dismiss { id, why, note } => {
            let mut s = Session::open(&env, "cli")?;
            println!(
                "{}",
                s.apply(&Op::Dismiss {
                    id,
                    why,
                    note,
                    reason: "dismissed from the cli".into()
                })?
            );
        }
        Cmd::Reopen { id, note } => {
            let mut s = Session::open(&env, "cli")?;
            println!(
                "{}",
                s.apply(&Op::Reopen {
                    id,
                    note,
                    reason: "reopened from the cli".into()
                })?
            );
        }
        Cmd::Digest { json } => {
            let s = Session::open(&env, "cli")?;
            let defaults = s.ledger.unacknowledged_defaults();
            let untriaged: Vec<&Item> = s
                .ledger
                .open()
                .into_iter()
                .filter(|i| {
                    i.status == Status::Raised && env.session.as_deref() != i.session_id.as_deref()
                })
                .collect();
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "defaults": defaults, "untriaged_from_other_sessions": untriaged })
                );
                return Ok(());
            }
            println!(
                "{} default(s) taken without you · {} untriaged item(s) from other sessions",
                defaults.len(),
                untriaged.len()
            );
            for i in defaults {
                let d = i.default.as_ref().expect("defaulted");
                println!(
                    "{} [{}] {} → took: {}",
                    i.id,
                    d.cause.as_str(),
                    clip(&i.question, 90),
                    clip(&d.option, 80)
                );
            }
            for i in untriaged {
                print!("{}", line(i));
            }
        }
        Cmd::Stats { json } => {
            let s = Session::open(&env, "cli")?;
            let h = Health::of(&s.ledger);
            if json {
                println!("{}", serde_json::to_string(&h)?);
            } else {
                let pct = |n: usize, d: usize| {
                    if d == 0 {
                        "—".to_owned()
                    } else {
                        format!("{:.0}%", 100.0 * n as f64 / d as f64)
                    }
                };
                println!(
                    "presented {} · action rate {} (target ≥60%) · not useful {} (target ≤10%) · overrides {} of {} · median time to decide {} · unqualified deferrals {} · unreviewed defaults {}",
                    h.presented,
                    pct(h.acted, h.presented),
                    pct(h.not_useful, h.presented),
                    h.chosen_against_recommendation,
                    h.chosen_with_recommendation_known,
                    h.median_seconds_to_decide
                        .map_or("—".into(), |s| format!("{s}s")),
                    h.deferred_unqualified,
                    h.defaults_unacknowledged
                );
                if !s.ledger.orphans.is_empty() {
                    println!(
                        "warning: {} event(s) for unknown items",
                        s.ledger.orphans.len()
                    );
                }
            }
        }
        Cmd::Scan { paths } => {
            let mut total = 0;
            for p in paths {
                let text = std::fs::read_to_string(&p)
                    .with_context(|| format!("reading {}", p.display()))?;
                let abs = if p.is_absolute() {
                    p.clone()
                } else {
                    env.cwd.join(&p)
                };
                for o in hooks::scan_text(&env, &abs.to_string_lossy(), &[text], "cli")? {
                    total += 1;
                    println!(
                        "{}",
                        match o {
                            ops::RaiseOutcome::New(id) => format!("raised {id}"),
                            ops::RaiseOutcome::Existing(id) => format!("{id} already raised"),
                            ops::RaiseOutcome::Duplicate(id) => format!("linked to open {id}"),
                        }
                    );
                }
            }
            if total == 0 {
                println!("nothing to raise");
            }
        }
        Cmd::Trailer { id } => {
            let s = Session::open(&env, "cli")?;
            let item = s
                .ledger
                .resolve(&id)
                .ok_or_else(|| anyhow!("no single item matches `{id}`"))?;
            let d = item
                .decision
                .as_ref()
                .ok_or_else(|| anyhow!("{} is not decided", item.id))?;
            println!("Weigh-In: {} decided {}", item.id, d.option);
        }
        Cmd::Board { cmd } => match cmd {
            BoardCmd::Export { out } => {
                let s = Session::open(&env, "cli")?;
                let scope = Scope {
                    repo: env.worktree.clone().unwrap_or_else(|| {
                        env.cwd
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    }),
                    session: env
                        .session
                        .clone()
                        .map_or_else(|| "all sessions".into(), |s| format!("session {s}")),
                    source: env.branch.clone().unwrap_or_default(),
                    generated_at: String::new(),
                    board_mode: env.config.board.mode.clone(),
                };
                if env.config.board.mode != "artifact" {
                    eprintln!(
                        "note: this repo's board mode is `{}`; do not publish its items to claude.ai",
                        env.config.board.mode
                    );
                }
                let state =
                    board::export(&s.ledger, scope, env.config.caps.board_focus, &env::now());
                let out = out.unwrap_or_else(|| env.state_dir().join("board-state.json"));
                if let Some(d) = out.parent() {
                    std::fs::create_dir_all(d)?;
                }
                std::fs::write(&out, serde_json::to_string_pretty(&state)?)?;
                let waiting = s
                    .ledger
                    .open()
                    .iter()
                    .filter(|i| i.needs_attention() && i.card.is_none())
                    .count();
                println!(
                    "wrote {} · {} card(s) · {} decided · {} default(s){}",
                    out.display(),
                    state.items.len(),
                    state.decided.len(),
                    state.defaults.len(),
                    if waiting > 0 {
                        format!(" · {waiting} open item(s) need grounding before they can be shown")
                    } else {
                        String::new()
                    }
                );
            }
            BoardCmd::Apply { path } => {
                let answers = read_answers(&path)?;
                let mut s = Session::open(&env, "cli")?;
                let mut applied = Vec::new();
                for (doc_id, a) in answers {
                    let op = match board::validate_answer(&s.ledger, &a) {
                        Ok(v) => to_op(&a.item_id, v),
                        Err(board::AnswerError::AlreadyApplied) => continue,
                        Err(e) => {
                            println!("✗ {doc_id}: {e}");
                            continue;
                        }
                    };
                    match s.apply(&op) {
                        Ok(r) => {
                            println!("✓ {r}");
                            applied.push(doc_id);
                        }
                        Err(e) => println!("✗ {doc_id}: {e:#}"),
                    }
                }
                println!(
                    "applied: {}",
                    if applied.is_empty() {
                        "none".into()
                    } else {
                        applied.join(" ")
                    }
                );
            }
        },
    }
    Ok(())
}

fn to_op(id: &str, v: ValidAnswer) -> Op {
    match v {
        ValidAnswer::Choose {
            option,
            note,
            seconds,
        } => Op::Decide {
            id: id.into(),
            option,
            note: Some(note),
            provenance: None,
            via: Some("board".into()),
            seconds,
        },
        ValidAnswer::Defer {
            trigger,
            resolve_by,
            default,
            note,
        } => Op::Defer {
            id: id.into(),
            flip: trigger.clone(),
            trigger,
            resolve_by,
            default,
            owner: None,
            unqualified: false,
            reason: if note.is_empty() {
                "deferred from the board".into()
            } else {
                note
            },
        },
        ValidAnswer::Dismiss { reason, note } => {
            let why = match reason.as_str() {
                "already-decided" => "already-decided:board".to_owned(),
                "duplicate" => "duplicate-of:board".to_owned(),
                r => r.to_owned(),
            };
            Op::Dismiss {
                id: id.into(),
                why,
                note: Some(note),
                reason: "dismissed on the board".into(),
            }
        }
        ValidAnswer::NotMyCall { who, note } => Op::Dismiss {
            id: id.into(),
            why: format!("not-my-call:{who}"),
            note: Some(note),
            reason: "handed off on the board".into(),
        },
        ValidAnswer::Reopen { note } => Op::Reopen {
            id: id.into(),
            note: Some(note),
            reason: "reopened on the board".into(),
        },
    }
}

/// Answers as `(document id, answer)`. Accepts one document, an array, `{ "id", "data" }` wrappers
/// as ArtifactData prints them, or a directory of such files.
fn read_answers(path: &std::path::Path) -> Result<Vec<(String, Answer)>> {
    let mut files = Vec::new();
    if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "json") {
                    files.push(p);
                }
            }
        }
    } else {
        files.push(path.to_path_buf());
    }
    let mut out = Vec::new();
    for f in files {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&f)?)
            .with_context(|| format!("parsing {}", f.display()))?;
        let docs = match v {
            serde_json::Value::Array(a) => a,
            other => vec![other],
        };
        for d in docs {
            let (id, data) = match (d.get("id").and_then(|x| x.as_str()), d.get("data")) {
                (Some(id), Some(data)) => (id.to_owned(), data.clone()),
                _ => (
                    f.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    d.clone(),
                ),
            };
            let mut a: Answer =
                serde_json::from_value(data).with_context(|| format!("answer {id}"))?;
            if a.item_id.is_empty() {
                a.item_id.clone_from(&id);
            }
            out.push((id, a));
        }
    }
    Ok(out)
}

fn clip(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= n {
        s
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

/// Two lines per item, about 60 tokens.
fn line(i: &Item) -> String {
    let mut meta = vec![
        i.door().as_str().to_owned(),
        i.authority().as_str().to_owned(),
        i.status.as_str().to_owned(),
        format!("{} ago", env::age(&i.raised_ts)),
    ];
    if i.channel != weighin_core::event::Channel::Marker {
        meta.push(i.channel.as_str().to_owned());
    }
    if !i.also_raised_in.is_empty() {
        meta.push(format!("raised in {} sessions", i.also_raised_in.len() + 1));
    }
    if i.card.is_some() {
        meta.push("card".into());
    }
    let mut s = format!(
        "{} [{}] {}\n",
        i.id,
        meta.join(" · "),
        clip(&i.question, 140)
    );
    let mut detail = Vec::new();
    if let Some(d) = &i.did {
        detail.push(format!("did: {}", clip(d, 70)));
    }
    if let Some(f) = &i.flip_hint {
        detail.push(format!("flip: {}", clip(f, 60)));
    }
    if let Some(a) = &i.at {
        detail.push(format!("at: {a}"));
    }
    if let Some(d) = &i.deferral {
        detail.push(format!(
            "waiting for: {} (default {} by {})",
            clip(&d.trigger, 50),
            d.default_option,
            d.resolve_by
        ));
    }
    if let Some(d) = &i.default {
        detail.push(format!("took: {}", clip(&d.option, 60)));
    }
    if !detail.is_empty() {
        s.push_str(&format!("  {}\n", detail.join(" · ")));
    }
    s
}
