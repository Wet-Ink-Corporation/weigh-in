//! `weighin`: the single writer of the Weigh-In decision ledger.
//!
//! Output is written for a model to read at the lowest token cost: short lines, no decoration,
//! `--json` where a structure is needed.

#![forbid(unsafe_code)]

mod annotate;
mod context;
mod env;
mod git;
mod hooks;
mod journal;
mod ops;
mod store;
mod text;

use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use std::io::{self, Read as _, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use weighin_core::board::{self, Answer, Scope, ValidAnswer};
use weighin_core::marker::{Door, parse_markers};
use weighin_core::projection::Health;
use weighin_core::{Item, Ledger, Status};

use crate::env::{Config, ConfigSource, Env};
use crate::ops::{DecideOp, DeferOp, Op, RaiseInput, Session, read_ledger};
use crate::text::Report;

// `help` and `about` attributes, not doc comments, carry text that names environment variables
// or tools: clap prints them verbatim, so they cannot take rustdoc's backticks.
#[derive(Parser)]
#[command(name = "weighin", version, about = "Weigh-In decision ledger")]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Session key (defaults to $WEIGHIN_SESSION, $CLAUDE_SESSION_ID, $CLAUDE_CODE_SESSION_ID)"
    )]
    session: Option<String>,
    /// Work as if run from this directory.
    #[arg(long, global = true)]
    dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Use this ledger file (wins over $WEIGHIN_STORE and the config's `store`)"
    )]
    store: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Hook entry point: reads the hook JSON on stdin.
    Hook {
        /// The event name `hooks.json` passes (B-16). Unused: the event is read from stdin.
        #[arg(hide = true)]
        _event: Option<String>,
    },
    #[command(flatten)]
    Ledger(LedgerCmd),
}

/// Every command but `hook`: these report their errors and exit 1 on failure (a closed stdout
/// aside, which exits 0; see `report`).
#[derive(Subcommand)]
enum LedgerCmd {
    /// Print the marker convention and open counts.
    Prime,
    /// Create .kb/_ledger/config.json.
    Init(InitArgs),
    /// List open items (this session by default when a session is known).
    Open(OpenArgs),
    /// Show one item in full.
    Show(ItemArgs),
    /// Script-built evidence for one item (about 600 tokens at most).
    Context(ItemArgs),
    /// Raise a question by hand, or every `weigh-in` block on stdin with --markers.
    Raise(RaiseArgs),
    /// Apply a JSON array of operations from stdin (or --file) in one call.
    Batch(BatchArgs),
    /// Record a decision.
    Decide(DecideArgs),
    /// Defer with a named fact, a trigger, a resolve-by and a default.
    Defer(DeferArgs),
    /// Dismiss: `not-useful`, `recoverable`, `moot`, `already-decided:<ref>`, `duplicate-of:<id>`, `not-my-call:<who>`.
    Dismiss(DismissArgs),
    /// Reopen a decided, defaulted or dismissed item.
    Reopen(ReopenArgs),
    /// Defaults taken without you, and unattended items not yet triaged.
    Digest(JsonArgs),
    /// Action rate, not-useful rate, overrides, time to decide.
    Stats(JsonArgs),
    /// The commit trailer for a decided item.
    Trailer(TrailerArgs),
    /// Raise every open question and new TODO in a document or source file (backfill).
    Scan(ScanArgs),
    /// Exchange with the Artifact board.
    Board {
        #[command(subcommand)]
        cmd: BoardCmd,
    },
    /// Every ledger event as JSON lines, for a committed journal (.kb/_ledger/journal/).
    Journal,
}

#[derive(Args)]
struct InitArgs {
    /// `own` or `client` (client repos default to a local board). Unchanged when not given; a
    /// first `init` is `own`.
    #[arg(long)]
    class: Option<String>,
    /// `artifact`, `local` or `off`. Unchanged when not given, unless `--class` is.
    #[arg(long)]
    board: Option<String>,
    #[arg(long)]
    board_url: Option<String>,
    #[arg(long)]
    owner: Option<String>,
    /// Keep the ledger here instead of the default (relative to the folder, or absolute).
    #[arg(long = "ledger")]
    ledger: Option<String>,
}

#[derive(Args)]
struct OpenArgs {
    #[command(flatten)]
    scope: OpenScope,
    #[arg(long, default_value_t = 12)]
    top: usize,
    /// Append a context pack for each listed item that has a path anchor.
    #[arg(long)]
    with_context: bool,
    #[arg(long)]
    json: bool,
}

/// Which items `open` lists, as its flags spell it; [`OpenScope::scope`] reads them as one
/// [`OpenFrom`].
#[derive(Args)]
struct OpenScope {
    /// Every open item in the repository, not only this session's.
    #[arg(long)]
    repo: bool,
    /// Every item, open or not (wins over `--repo`).
    #[arg(long)]
    all: bool,
}

/// Which items `open` lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenFrom {
    /// The open items of this session, or of the repository when no session is known.
    Session,
    /// The open items of the repository.
    Repo,
    /// Every item, open or not.
    All,
}

impl OpenScope {
    /// The one scope the flags name: `--all` wins over `--repo`, and neither is this session.
    fn scope(&self) -> OpenFrom {
        if self.all {
            OpenFrom::All
        } else if self.repo {
            OpenFrom::Repo
        } else {
            OpenFrom::Session
        }
    }
}

#[derive(Args)]
struct ItemArgs {
    id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RaiseArgs {
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
}

#[derive(Args)]
struct BatchArgs {
    #[arg(long)]
    file: Option<PathBuf>,
}

#[derive(Args)]
struct DecideArgs {
    id: String,
    option: String,
    #[arg(long)]
    note: Option<String>,
    #[arg(long, default_value = "cli")]
    via: String,
}

#[derive(Args)]
struct DeferArgs {
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
}

#[derive(Args)]
struct DismissArgs {
    id: String,
    why: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Args)]
struct ReopenArgs {
    id: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Args)]
struct JsonArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct TrailerArgs {
    id: String,
}

#[derive(Args)]
struct ScanArgs {
    paths: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum BoardCmd {
    #[command(about = "Write the board state document (push it with ArtifactData set, file_path)")]
    Export(ExportArgs),
    /// Apply answers read back from the board (a JSON file, an array, or a directory of documents).
    Apply(ApplyArgs),
}

#[derive(Args)]
struct ExportArgs {
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct ApplyArgs {
    path: PathBuf,
}

fn main() -> ExitCode {
    let Cli {
        session,
        dir,
        store,
        cmd,
    } = Cli::parse();
    let cmd = match cmd {
        Cmd::Hook { .. } => return run_hook_guarded(),
        Cmd::Ledger(cmd) => cmd,
    };
    let stdout = io::stdout();
    let mut out = Stdout(stdout.lock());
    let globals = Globals {
        session,
        dir,
        store,
    };
    match run(globals, cmd, &mut out).and_then(|()| out.flush().map_err(anyhow::Error::from)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => report(&e),
    }
}

/// The options every command takes.
struct Globals {
    session: Option<String>,
    dir: Option<PathBuf>,
    store: Option<PathBuf>,
}

/// `weighin: <error chain>` on stderr and exit 1; a closed stdout (`weighin open | head`) is
/// not a failure, so it exits 0 without a word.
fn report(e: &anyhow::Error) -> ExitCode {
    if is_closed_stdout(e) {
        return ExitCode::SUCCESS;
    }
    match writeln!(io::stderr().lock(), "weighin: {e:#}") {
        // With stderr gone as well, the exit code is the only report left.
        Ok(()) | Err(_) => ExitCode::FAILURE,
    }
}

/// The command's standard output. A broken pipe on it is tagged [`StdoutClosed`], so `report`
/// tells a reader that stopped reading from any other broken pipe (stderr, a child process).
struct Stdout<W>(W);

impl<W: Write> Write for Stdout<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf).map_err(StdoutClosed::tag)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush().map_err(StdoutClosed::tag)
    }
}

/// A broken pipe on standard output: whoever read it has stopped reading.
#[derive(Debug)]
struct StdoutClosed(io::Error);

impl StdoutClosed {
    fn tag(e: io::Error) -> io::Error {
        if e.kind() == io::ErrorKind::BrokenPipe {
            io::Error::new(io::ErrorKind::BrokenPipe, Self(e))
        } else {
            e
        }
    }
}

impl std::fmt::Display for StdoutClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "writing standard output: {}", self.0)
    }
}

impl std::error::Error for StdoutClosed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

fn is_closed_stdout(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
            .is_some_and(<dyn std::error::Error + Send + Sync>::is::<StdoutClosed>)
    })
}

/// A hook never fails a session, not even on a bug: a panic prints nothing and exits 0.
#[expect(
    clippy::disallowed_methods,
    reason = "process-boundary fault barrier: a hook never fails a session, even on a panic (AGENTS.md)"
)]
fn run_hook_guarded() -> ExitCode {
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(|| {
        hooks::entry(&mut io::stdin().lock(), &mut io::stdout().lock());
    });
    match outcome {
        // `hooks::entry` logs its own failures; a panic went to the silent panic hook above.
        // Either way the session goes on.
        Ok(()) | Err(_) => ExitCode::SUCCESS,
    }
}

fn stdin() -> Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn run(g: Globals, cmd: LedgerCmd, out: &mut impl Write) -> Result<()> {
    let cwd = match g.dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let env = Env::discover(&cwd, g.session).with_store(g.store);
    // Only hooks run on the defaults when the config is broken: a command would drop its
    // settings, the ledger's place among them, without a word.
    if let ConfigSource::Broken(e) = &env.config_source {
        bail!("loading this folder's config: {}", Report(e));
    }
    match cmd {
        LedgerCmd::Prime => Ok(write!(out, "{}", hooks::prime(&env, "manual")?)?),
        LedgerCmd::Init(a) => init(env, a, out),
        LedgerCmd::Open(a) => open(&env, &a, out),
        LedgerCmd::Show(a) => show(&env, &a, out),
        LedgerCmd::Context(a) => context_pack(&env, &a, out),
        LedgerCmd::Raise(a) => raise(&env, a, out),
        LedgerCmd::Batch(a) => batch(&env, a, out),
        LedgerCmd::Decide(a) => apply_one(&env, &a.into_op(), out),
        LedgerCmd::Defer(a) => apply_one(&env, &a.into_op(), out),
        LedgerCmd::Dismiss(a) => apply_one(&env, &a.into_op(), out),
        LedgerCmd::Reopen(a) => apply_one(&env, &a.into_op(), out),
        LedgerCmd::Digest(a) => digest(&env, &a, out),
        LedgerCmd::Stats(a) => stats(&env, &a, out),
        LedgerCmd::Trailer(a) => trailer(&env, &a, out),
        LedgerCmd::Scan(a) => scan(&env, a, out),
        LedgerCmd::Board {
            cmd: BoardCmd::Export(a),
        } => board_export(&env, a, out),
        LedgerCmd::Board {
            cmd: BoardCmd::Apply(a),
        } => board_apply(&env, &a, out),
        LedgerCmd::Journal => journal::write(&ops::stored_events(&env)?, out),
    }
}

fn init(mut env: Env, a: InitArgs, out: &mut impl Write) -> Result<()> {
    let c = &mut env.config;
    // Only what was given changes: a later `init --ledger` keeps a client repo's local board.
    match (a.class, a.board) {
        (class, Some(board)) => {
            if let Some(class) = class {
                c.repo_class = class;
            }
            c.board.mode = board;
        }
        (Some(class), None) => {
            c.board.mode = if class == "client" {
                "local"
            } else {
                "artifact"
            }
            .into();
            c.repo_class = class;
        }
        (None, None) => {}
    }
    if a.board_url.is_some() {
        c.board.url = a.board_url;
    }
    if let Some(o) = a.owner {
        c.owner = o;
    }
    if let Some(l) = a.ledger {
        c.store = Some(l).filter(|l| !l.trim().is_empty());
    }
    let p = env.config.save(&env.root())?;
    writeln!(
        out,
        "wrote {} · class {} · board {} · ledger {}",
        env.relative(&p.to_string_lossy()),
        env.config.repo_class,
        env.config.board.mode,
        env.store_path().display()
    )?;
    Ok(())
}

fn open(env: &Env, a: &OpenArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let from = a.scope.scope();
    let scope_session = match from {
        OpenFrom::Session => env.session.as_deref(),
        OpenFrom::Repo | OpenFrom::All => None,
    };
    let items: Vec<&Item> = match (from, scope_session) {
        (OpenFrom::All, _) => ledger.items.values().collect(),
        (OpenFrom::Session, Some(id)) => ledger.open_in_session(id),
        (OpenFrom::Session | OpenFrom::Repo, _) => ledger.open(),
    };
    if a.json {
        writeln!(
            out,
            "{}",
            serde_json::to_string(&items.iter().take(a.top).collect::<Vec<_>>())?
        )?;
        return Ok(());
    }
    open_summary(&ledger, scope_session, out)?;
    if items.is_empty() {
        writeln!(out, "No decisions need you.")?;
    }
    for i in items.iter().take(a.top) {
        write_line(out, i)?;
    }
    if items.len() > a.top {
        writeln!(out, "… {} more (--top)", items.len() - a.top)?;
    }
    if a.with_context {
        for i in items
            .iter()
            .take(a.top)
            .filter(|i| i.needs_attention() && i.at.is_some())
        {
            writeln!(
                out,
                "{}",
                context::render(&context::build(env, i), env.config.caps.context_chars)
            )?;
        }
    }
    Ok(())
}

/// The counts line `open` starts with.
fn open_summary(ledger: &Ledger, session: Option<&str>, out: &mut impl Write) -> Result<()> {
    let repo_open = ledger.open();
    let one_way = repo_open
        .iter()
        .filter(|i| i.door() == Door::OneWay)
        .count();
    let defaults = ledger.unacknowledged_defaults().len();
    let here = session.map_or_else(String::new, |id| {
        format!(
            "{} open in this session · ",
            ledger.open_in_session(id).len()
        )
    });
    writeln!(
        out,
        "{here}{} open in repo ({one_way} one-way) · {defaults} default(s) to review",
        repo_open.len()
    )?;
    Ok(())
}

fn show(env: &Env, a: &ItemArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let item = ledger
        .resolve(&a.id)
        .ok_or_else(|| anyhow!("no single item matches `{}`", a.id))?;
    if a.json {
        writeln!(out, "{}", serde_json::to_string_pretty(item)?)?;
        return Ok(());
    }
    write_line(out, item)?;
    if let Some(c) = &item.card {
        writeln!(out, "  card: {}", serde_json::to_string(c)?)?;
    }
    if let Some(d) = &item.decision {
        writeln!(
            out,
            "  decided {} by {} ({}) via {} · {}",
            d.option,
            d.decider,
            d.provenance.as_str(),
            d.via,
            d.ts
        )?;
    }
    for r in &item.records {
        writeln!(out, "  record: {} ({})", r.path, r.atom_id)?;
    }
    Ok(())
}

fn context_pack(env: &Env, a: &ItemArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let item = ledger
        .resolve(&a.id)
        .ok_or_else(|| anyhow!("no single item matches `{}`", a.id))?;
    let pack = context::build(env, item);
    if a.json {
        writeln!(out, "{}", serde_json::to_string(&pack)?)?;
    } else {
        writeln!(
            out,
            "{}",
            context::render(&pack, env.config.caps.context_chars)
        )?;
    }
    Ok(())
}

fn raise(env: &Env, a: RaiseArgs, out: &mut impl Write) -> Result<()> {
    // Parsed before the session opens, so input with no blocks leaves no ledger behind (B-01).
    let inputs = raise_inputs(a)?;
    if inputs.is_empty() {
        bail!("no weigh-in blocks found on stdin");
    }
    let mut s = Session::open(env, "cli")?;
    let mut receipts = Receipts::new(out);
    let outcome = inputs.into_iter().try_for_each(|r| {
        receipts.line(format_args!("{}", s.apply(&Op::Raise(r))?));
        Ok(())
    });
    receipts.finish(outcome)
}

/// The receipts of a command that changes the ledger, one line per change. A failed write stops
/// the receipts but not the changes: every operation is still applied, and [`Receipts::finish`]
/// returns the failure at the end, so a reader that stops early (`weighin batch | head -1`)
/// never leaves a batch half applied.
struct Receipts<W> {
    out: W,
    /// The first write that failed; nothing more is written after it.
    failed: Option<io::Error>,
}

impl<W: Write> Receipts<W> {
    fn new(out: W) -> Self {
        Self { out, failed: None }
    }

    /// Write one receipt line, unless an earlier one failed.
    fn line(&mut self, line: std::fmt::Arguments<'_>) {
        if self.failed.is_none()
            && let Err(e) = writeln!(self.out, "{line}")
        {
            self.failed = Some(e);
        }
    }

    /// The command's result: its own failure first, since the changes matter more than their
    /// receipts, then the write that failed.
    ///
    /// # Errors
    /// Fails when `outcome` is a failure or a receipt could not be written.
    fn finish(self, outcome: Result<()>) -> Result<()> {
        match (outcome, self.failed) {
            (Ok(()), None) => Ok(()),
            (Ok(()), Some(written)) => Err(written.into()),
            (Err(e), None) => Err(e),
            // Not chained: a closed stdout in the chain would make `report` exit 0 in silence.
            (Err(e), Some(written)) => Err(anyhow!("{e:#}; the receipts stopped at {written}")),
        }
    }
}

/// What `raise` raises: every `weigh-in` block on stdin with `--markers`, else the one question.
fn raise_inputs(a: RaiseArgs) -> Result<Vec<RaiseInput>> {
    if a.markers {
        let text = stdin()?;
        let ch = if a.channel == "manual" {
            "sweep".to_owned()
        } else {
            a.channel
        };
        return Ok(parse_markers(&text)
            .iter()
            .map(|m| {
                let mut r = RaiseInput::from_marker(m, weighin_core::event::Channel::Sweep);
                // Each input owns its channel.
                r.channel = Some(ch.clone());
                r
            })
            .collect());
    }
    let q =
        a.q.ok_or_else(|| anyhow!("--q is required (or --markers with blocks on stdin)"))?;
    Ok(vec![RaiseInput {
        q,
        kind: a.kind,
        door: a.door,
        authority: a.authority,
        did: a.did,
        flip: a.flip,
        at: a.at,
        channel: Some(a.channel),
        ..Default::default()
    }])
}

fn batch(env: &Env, a: BatchArgs, out: &mut impl Write) -> Result<()> {
    let text = match a.file {
        Some(f) => {
            std::fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?
        }
        None => stdin()?,
    };
    let ops = ops::parse_ops(&text)?;
    let total = ops.len();
    let mut s = Session::open(env, "cli")?;
    let mut receipts = Receipts::new(out);
    let mut failed = 0;
    for (n, op) in ops.into_iter().enumerate() {
        match s.apply_parsed(op) {
            Ok(r) => receipts.line(format_args!("✓ {r}")),
            Err(e) => {
                failed += 1;
                receipts.line(format_args!("✗ op {n}: {e:#}"));
            }
        }
    }
    let outcome = if failed > 0 {
        Err(anyhow!(
            "{failed} of {total} operations failed; the rest were applied"
        ))
    } else {
        Ok(())
    };
    receipts.finish(outcome)
}

/// One operation through the write path, and its receipt.
fn apply_one(env: &Env, op: &Op, out: &mut impl Write) -> Result<()> {
    let mut s = Session::open(env, "cli")?;
    writeln!(out, "{}", s.apply(op)?)?;
    Ok(())
}

impl DecideArgs {
    fn into_op(self) -> Op {
        Op::Decide(DecideOp {
            id: self.id,
            option: self.option,
            note: self.note,
            provenance: None,
            via: Some(self.via),
            seconds: None,
        })
    }
}

impl DeferArgs {
    fn into_op(self) -> Op {
        Op::Defer(DeferOp {
            id: self.id,
            flip: self.flip,
            trigger: self.trigger,
            resolve_by: self.resolve_by,
            default: self.default,
            owner: None,
            unqualified: false,
            reason: self
                .reason
                .unwrap_or_else(|| "deferred with a named fact".into()),
        })
    }
}

impl DismissArgs {
    fn into_op(self) -> Op {
        Op::Dismiss {
            id: self.id,
            why: self.why,
            note: self.note,
            reason: "dismissed from the cli".into(),
        }
    }
}

impl ReopenArgs {
    fn into_op(self) -> Op {
        Op::Reopen {
            id: self.id,
            note: self.note,
            reason: "reopened from the cli".into(),
        }
    }
}

/// `digest --json`: the defaults and the untriaged items.
#[derive(serde::Serialize)]
struct DigestJson<'l> {
    defaults: &'l [&'l Item],
    untriaged_from_other_sessions: &'l [&'l Item],
}

/// The `digest --json` line. It goes through `serde_json::Value`, whose maps are sorted, so
/// the keys at every level, items included, come out in alphabetical order.
fn digest_json(defaults: &[&Item], untriaged: &[&Item]) -> serde_json::Result<String> {
    let doc = DigestJson {
        defaults,
        untriaged_from_other_sessions: untriaged,
    };
    serde_json::to_string(&serde_json::to_value(&doc)?)
}

fn digest(env: &Env, a: &JsonArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let defaults = ledger.unacknowledged_defaults();
    let untriaged: Vec<&Item> = ledger
        .open()
        .into_iter()
        .filter(|i| i.status == Status::Raised && env.session.as_deref() != i.session_id.as_deref())
        .collect();
    if a.json {
        writeln!(out, "{}", digest_json(&defaults, &untriaged)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "{} default(s) taken without you · {} untriaged item(s) from other sessions",
        defaults.len(),
        untriaged.len()
    )?;
    for i in defaults {
        // `unacknowledged_defaults` only returns items with a default.
        let Some(d) = i.default.as_ref() else {
            continue;
        };
        writeln!(
            out,
            "{} [{}] {} → took: {}",
            i.id,
            d.cause.as_str(),
            clip(&i.question, 90),
            clip(&d.option, 80)
        )?;
    }
    for i in untriaged {
        write_line(out, i)?;
    }
    Ok(())
}

/// The health numbers over the ledger and every committed journal (`journal.rs`).
fn stats(env: &Env, a: &JsonArgs, out: &mut impl Write) -> Result<()> {
    let ledger = journal::pool(ops::stored_events(env)?, journal::read(&env.root())?)?;
    let h = Health::of(&ledger);
    if a.json {
        writeln!(out, "{}", serde_json::to_string(&h)?)?;
        return Ok(());
    }
    writeln!(
        out,
        "presented {} · action rate {} (target ≥60%) · not useful {} (target ≤10%) · overrides {} of {} · median time to decide {} · unqualified deferrals {} · unreviewed defaults {}",
        h.presented,
        percent(h.acted, h.presented),
        percent(h.not_useful, h.presented),
        h.chosen_against_recommendation,
        h.chosen_with_recommendation_known,
        h.median_seconds_to_decide
            .map_or_else(|| "—".to_owned(), |s| format!("{s}s")),
        h.deferred_unqualified,
        h.defaults_unacknowledged
    )?;
    if !ledger.orphans.is_empty() {
        writeln!(
            out,
            "warning: {} event(s) for unknown items",
            ledger.orphans.len()
        )?;
    }
    Ok(())
}

/// `n` of `d` as a whole percentage, rounded half to even as `{:.0}` rounds a float, or `—`
/// when there is nothing to divide by (or the count is too large to scale).
fn percent(n: usize, d: usize) -> String {
    rounded_percent(n, d).map_or_else(|| "—".to_owned(), |p| format!("{p}%"))
}

/// `None` when `d` is zero or `n * 100` overflows `usize`.
fn rounded_percent(n: usize, d: usize) -> Option<usize> {
    let scaled = n.checked_mul(100)?;
    let quotient = scaled.checked_div(d)?;
    let remainder = scaled.checked_rem(d)?;
    // `remainder < d`, so `d - remainder` cannot underflow; compare halves without doubling.
    let rest = d.checked_sub(remainder)?;
    let up = remainder > rest || (remainder == rest && quotient % 2 == 1);
    quotient.checked_add(usize::from(up))
}

fn scan(env: &Env, a: ScanArgs, out: &mut impl Write) -> Result<()> {
    let mut receipts = Receipts::new(out);
    let mut total = 0;
    let outcome = a.paths.into_iter().try_for_each(|p| {
        // Relative to `--dir`, for the text read and for the anchor alike.
        let abs = if p.is_absolute() { p } else { env.cwd.join(&p) };
        let text =
            std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
        for o in hooks::scan_text(env, &abs.to_string_lossy(), &[text.as_str()], "cli")? {
            total += 1;
            match o {
                ops::RaiseOutcome::New(id) => receipts.line(format_args!("raised {id}")),
                ops::RaiseOutcome::Existing(id) => {
                    receipts.line(format_args!("{id} already raised"));
                }
                ops::RaiseOutcome::Duplicate(id) => {
                    receipts.line(format_args!("linked to open {id}"));
                }
            }
        }
        Ok(())
    });
    if outcome.is_ok() && total == 0 {
        receipts.line(format_args!("nothing to raise"));
    }
    receipts.finish(outcome)
}

fn trailer(env: &Env, a: &TrailerArgs, out: &mut impl Write) -> Result<()> {
    let ledger = read_ledger(env)?;
    let item = ledger
        .resolve(&a.id)
        .ok_or_else(|| anyhow!("no single item matches `{}`", a.id))?;
    let d = item
        .decision
        .as_ref()
        .ok_or_else(|| anyhow!("{} is not decided", item.id))?;
    writeln!(
        out,
        "{}",
        ops::Trailer {
            id: &item.id,
            option: &d.option
        }
    )?;
    Ok(())
}

fn board_export(env: &Env, a: ExportArgs, out: &mut impl Write) -> Result<()> {
    // The default state file sits beside the ledger; writing it would create the ledger's
    // directory where no item was ever raised. Only a stored ledger has items to keep off
    // claude.ai, so only its export asks whether this folder may publish.
    let ledger = match (ops::stored_ledger(env)?, &a.out) {
        (Some(ledger), _) => {
            publishable(env)?;
            ledger
        }
        (None, Some(_)) => Ledger::default(),
        (None, None) => {
            writeln!(
                out,
                "no ledger at {} yet: nothing to export",
                env.store_path().display()
            )?;
            return Ok(());
        }
    };
    let state = board::export(
        &ledger,
        board_scope(env),
        env.config.caps.board_focus,
        &env::now(),
    );
    let path = a
        .out
        .unwrap_or_else(|| env.state_dir().join("board-state.json"));
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&state)?)?;
    let waiting = ledger
        .open()
        .iter()
        .filter(|i| i.needs_attention() && i.card.is_none())
        .count();
    writeln!(
        out,
        "wrote {} · {} card(s) · {} decided · {} default(s){}",
        path.display(),
        state.items.len(),
        state.decided.len(),
        state.defaults.len(),
        if waiting > 0 {
            format!(" · {waiting} open item(s) need grounding before they can be shown")
        } else {
            String::new()
        }
    )?;
    Ok(())
}

/// Whether this folder's items may go to the claude.ai board: only when its config chose the
/// `artifact` board (decision wi-7c21e4). A folder never initialised may be a client's, so the
/// defaults do not count as a choice.
fn publishable(env: &Env) -> Result<()> {
    match &env.config_source {
        ConfigSource::File => {}
        ConfigSource::Absent => bail!(
            "no {} here, so no board mode was chosen: run `weighin init --class own`, or \
             `weighin init --class client` for a client's repository (its board stays local), \
             before exporting the board",
            Config::PATH
        ),
        ConfigSource::Broken(e) => bail!("not exporting the board: {}", Report(e)),
    }
    let mode = &env.config.board.mode;
    if mode != "artifact" {
        bail!(
            "this repo's board mode is `{mode}`: its items are not published to claude.ai, so \
             there is no board to export (`weighin init --board artifact` changes the mode)"
        );
    }
    Ok(())
}

/// Where the board's items come from, as the board shows it.
fn board_scope(env: &Env) -> Scope {
    Scope {
        repo: env.worktree.clone().unwrap_or_else(|| {
            env.cwd
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        }),
        session: env
            .session
            .as_deref()
            .map_or_else(|| "all sessions".into(), |s| format!("session {s}")),
        source: env.branch.clone().unwrap_or_default(),
        generated_at: String::new(),
        board_mode: env.config.board.mode.clone(),
    }
}

fn board_apply(env: &Env, a: &ApplyArgs, out: &mut impl Write) -> Result<()> {
    let answers = read_answers(&a.path)?;
    let mut s = Session::open(env, "cli")?;
    let mut receipts = Receipts::new(out);
    let mut applied = Vec::new();
    for (doc_id, answer) in answers {
        let op = match board::validate_answer(&s.ledger, &answer) {
            Ok(v) => to_op(&answer.item_id, v),
            Err(board::AnswerError::AlreadyApplied) => continue,
            Err(e) => {
                receipts.line(format_args!("✗ {doc_id}: {e}"));
                continue;
            }
        };
        match s.apply(&op) {
            Ok(r) => {
                receipts.line(format_args!("✓ {r}"));
                applied.push(doc_id);
            }
            Err(e) => receipts.line(format_args!("✗ {doc_id}: {e:#}")),
        }
    }
    receipts.line(format_args!(
        "applied: {}",
        if applied.is_empty() {
            "none".into()
        } else {
            applied.join(" ")
        }
    ));
    receipts.finish(Ok(()))
}

fn to_op(id: &str, v: ValidAnswer) -> Op {
    match v {
        ValidAnswer::Choose {
            option,
            note,
            seconds,
        } => Op::Decide(DecideOp {
            id: id.into(),
            option,
            note: Some(note),
            provenance: None,
            via: Some("board".into()),
            seconds,
        }),
        ValidAnswer::Defer {
            trigger,
            resolve_by,
            default,
            note,
        } => Op::Defer(DeferOp {
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
        }),
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
/// as `ArtifactData` prints them, or a directory of such files.
fn read_answers(path: &std::path::Path) -> Result<Vec<(String, Answer)>> {
    let mut files = Vec::new();
    if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let p = e.path();
                // The entry's own type: a symlinked directory is not descended into, so a link
                // back up the tree cannot loop (H-13).
                let is_dir = e
                    .file_type()
                    .with_context(|| format!("reading {}", p.display()))?
                    .is_dir();
                if is_dir {
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

/// `s` on one line, cut to at most `n` characters.
fn clip(s: &str, n: usize) -> String {
    text::clip(&s.replace('\n', " "), n)
}

/// Two lines per item, about 60 tokens.
fn write_line(out: &mut impl Write, i: &Item) -> io::Result<()> {
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
    writeln!(
        out,
        "{} [{}] {}",
        i.id,
        meta.join(" · "),
        clip(&i.question, 140)
    )?;
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
        writeln!(out, "  {}", detail.join(" · "))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BatchArgs, Stdout, batch, clip, digest_json, is_closed_stdout, percent};
    use crate::env::{Config, ConfigSource, Env};
    use crate::ops::read_ledger;
    use anyhow::Context as _;
    use std::io::{self, Write as _};
    use weighin_core::event::{Channel, Raised};
    use weighin_core::marker::{Authority, Door, Kind};
    use weighin_core::{EventEnvelope, ItemId, Ledger, LedgerEvent};

    fn raised_ledger() -> Ledger {
        let id = ItemId::mint("s1", "Where should the ledger live?");
        let mut l = Ledger::default();
        l.apply(
            1,
            &EventEnvelope {
                event: LedgerEvent::ItemRaised(Raised {
                    item_id: id,
                    kind: Kind::Question,
                    question: "Where should the ledger live?".into(),
                    did: None,
                    flip: None,
                    at: None,
                    sym: None,
                    blocks: None,
                    door_hint: Door::TwoWay,
                    authority_hint: Authority::Authority,
                    channel: Channel::Marker,
                    quote: "Where should the ledger live?".into(),
                    source_path: None,
                    source_line: None,
                }),
                actor: "test".into(),
                session_id: Some("s1".into()),
                worktree: None,
                branch: None,
                head: None,
                ts: "2026-09-26T00:00:00Z".into(),
            },
        );
        l
    }

    #[test]
    #[cfg(unix)]
    #[cfg_attr(miri, ignore = "makes directories and symlinks on the file system")]
    fn answers_under_a_symlinked_directory_loop_are_read_once() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let answer = r#"{"item_id": "wi-000001", "action": "reopen"}"#;
        std::fs::write(dir.path().join("a.json"), answer)?;
        // One link, so a walk that follows it fails fast (on duplicates, at the kernel's 40-link
        // limit) rather than hanging as two links make it.
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop"))?;

        let answers = super::read_answers(dir.path())?;

        let ids: Vec<&str> = answers.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["a"]);
        Ok(())
    }

    #[test]
    fn digest_json_prints_every_key_sorted_as_a_json_value_does() {
        let ledger = raised_ledger();
        let untriaged = ledger.open();
        assert_eq!(untriaged.len(), 1);
        let printed = digest_json(&[], &untriaged).unwrap();
        let expected =
            serde_json::json!({ "defaults": [], "untriaged_from_other_sessions": untriaged })
                .to_string();
        assert_eq!(printed, expected);
        assert!(
            printed.starts_with(
                r#"{"defaults":[],"untriaged_from_other_sessions":[{"also_raised_in":[],"at":null,"#
            ),
            "{printed}"
        );
    }

    #[test]
    fn percent_rounds_half_to_even_like_float_formatting() {
        assert_eq!(percent(1, 8), "12%");
        assert_eq!(percent(3, 8), "38%");
        assert_eq!(percent(1, 3), "33%");
        assert_eq!(percent(2, 3), "67%");
        assert_eq!(percent(5, 5), "100%");
        assert_eq!(percent(0, 4), "0%");
    }

    #[test]
    fn percent_of_nothing_is_a_dash() {
        assert_eq!(percent(0, 0), "—");
        assert_eq!(percent(3, 0), "—");
    }

    #[test]
    fn percent_matches_zero_decimal_float_formatting_for_small_ratios() {
        for d in 1..=400_u32 {
            for n in 0..=d {
                let float = format!("{:.0}%", 100.0 * f64::from(n) / f64::from(d));
                let n = usize::try_from(n).unwrap();
                let d = usize::try_from(d).unwrap();
                assert_eq!(percent(n, d), float, "{n}/{d}");
            }
        }
    }

    #[test]
    fn clip_keeps_short_text_and_ellipsizes_long_text() {
        assert_eq!(clip("a\nb", 3), "a b");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("abc", 0), "…");
    }

    /// A writer whose reader has gone.
    struct Gone;

    impl io::Write for Gone {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn a_closed_stdout_anywhere_in_the_chain_is_recognised() {
        let written: anyhow::Result<()> = writeln!(Stdout(Gone), "an item").map_err(Into::into);
        let wrapped = written.context("writing the list").unwrap_err();
        assert!(is_closed_stdout(&wrapped));
        let flushed: anyhow::Result<()> = Stdout(Gone).flush().map_err(Into::into);
        assert!(is_closed_stdout(&flushed.unwrap_err()));
    }

    #[test]
    fn a_broken_pipe_elsewhere_is_a_failure() {
        let stderr: anyhow::Result<()> = writeln!(Gone, "note").map_err(Into::into);
        let wrapped = stderr.context("writing the note").unwrap_err();
        assert!(!is_closed_stdout(&wrapped));
        let other = anyhow::Error::from(io::Error::from(io::ErrorKind::NotFound));
        assert!(!is_closed_stdout(&other));
    }

    /// A standard output whose reader stops after the first line.
    struct OneLine {
        written: Vec<u8>,
    }

    impl io::Write for OneLine {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.written.contains(&b'\n') {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn a_batch_whose_reader_stops_early_still_applies_every_operation() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let env = Env {
            cwd: dir.path().to_path_buf(),
            repo_root: None,
            common_dir: None,
            branch: None,
            head: None,
            worktree: None,
            session: Some("s1".into()),
            config: Config::default(),
            config_source: ConfigSource::Absent,
            store_override: Some(dir.path().join("ledger.sqlite3")),
        };
        let file = dir.path().join("batch.json");
        std::fs::write(
            &file,
            r#"[{"op":"raise","q":"one?"},{"op":"raise","q":"two?"},{"op":"raise","q":"three?"}]"#,
        )?;
        let mut out = Stdout(OneLine {
            written: Vec::new(),
        });

        let result = batch(&env, BatchArgs { file: Some(file) }, &mut out);

        let stopped = result.expect_err("the closed stdout is reported");
        assert!(is_closed_stdout(&stopped), "{stopped:#}");
        let printed = String::from_utf8(out.0.written)?;
        assert_eq!(printed.lines().count(), 1, "{printed}");
        assert_eq!(read_ledger(&env)?.open().len(), 3);
        Ok(())
    }
}
