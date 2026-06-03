mod format;
mod init;
mod store;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use format::{lint, LintError, Status};
use store::{Store, TagOp};

#[derive(Parser)]
#[command(name = "pst", version, about = "plain-simple-tickets")]
struct Cli {
    /// DB file path (overrides $PST_FILE; default .pst/tickets)
    #[arg(long, global = true)]
    file: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold .pst/ here and activate pst for the listed (or auto-detected) agents
    Init {
        /// Print what pst has installed in this repo; exit
        #[arg(long, conflicts_with_all = ["uninstall", "cursor", "antigravity", "claude", "codex", "copilot"])]
        show: bool,
        /// Remove pst-owned files (data in .pst/ is preserved); exit
        #[arg(long, conflicts_with_all = ["show", "cursor", "antigravity", "claude", "codex", "copilot"])]
        uninstall: bool,
        #[arg(long)]
        cursor: bool,
        #[arg(long)]
        antigravity: bool,
        #[arg(long)]
        claude: bool,
        #[arg(long)]
        codex: bool,
        #[arg(long)]
        copilot: bool,
    },
    /// Append a new ticket; prints its number
    Add {
        body: String,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, default_value = "open")]
        status: String,
    },
    /// Edit ticket N (--tag +x adds, --tag -x removes)
    Set {
        n: usize,
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "tag", allow_hyphen_values = true)]
        tags: Vec<String>,
        #[arg(long)]
        body: Option<String>,
    },
    /// Close ticket N (tombstone)
    Close { n: usize },
    /// Reopen ticket N
    Reopen { n: usize },
    /// Mark ticket N work-in-progress
    Wip { n: usize },
    /// Validate the whole DB file
    Lint,
    /// Print one or more tickets plus their detail files; ids may be #-prefixed
    Show {
        #[arg(required = true, value_name = "N")]
        ids: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = cli
        .file
        .or_else(|| std::env::var_os("PST_FILE").map(PathBuf::from))
        .unwrap_or_else(|| ".pst/tickets".into());
    let store = Store::new(&path);

    let result = match cli.cmd {
        Cmd::Lint => return run_lint(&path),
        Cmd::Init { show, uninstall, cursor, antigravity, claude, codex, copilot } => {
            let mut tools = Vec::new();
            if cursor { tools.push(init::Tool::Cursor); }
            if antigravity { tools.push(init::Tool::Antigravity); }
            if claude { tools.push(init::Tool::Claude); }
            if codex { tools.push(init::Tool::Codex); }
            if copilot { tools.push(init::Tool::Copilot); }
            return match init::init(&init::Opts { tools, show, uninstall }) {
                Ok(code) => code,
                Err(e) => { eprintln!("pst: {e}"); ExitCode::FAILURE }
            };
        }
        Cmd::Add { body, tags, status } => parse_status(&status)
            .and_then(|s| store.add(s, &tags, &body).map_err(|e| e.to_string()))
            .map(|n| println!("{n}")),
        Cmd::Set { n, status, tags, body } => set(&store, n, status, &tags, body.as_deref()),
        Cmd::Close { n } => store.set_status(n, Status::Closed).map_err(|e| e.to_string()),
        Cmd::Reopen { n } => store.set_status(n, Status::Open).map_err(|e| e.to_string()),
        Cmd::Wip { n } => store.set_status(n, Status::Wip).map_err(|e| e.to_string()),
        Cmd::Show { ids } => return run_show(&store, &ids),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pst: {e}");
            ExitCode::FAILURE
        }
    }
}

fn set(store: &Store, n: usize, status: Option<String>, tags: &[String], body: Option<&str>) -> Result<(), String> {
    let status = status.as_deref().map(parse_status).transpose()?;
    let ops = tag_ops(tags)?;
    match (status, ops.is_empty(), body.is_none()) {
        (None, true, true) => Err("nothing to set; pass --status, --tag, and/or --body".into()),
        (Some(s), true, true) => store.set_status(n, s).map_err(|e| e.to_string()), // O(1) fast path
        _ => store.rewrite_line(n, status, &ops, body).map_err(|e| e.to_string()),
    }
}

/// Render each requested ticket, separated by a blank line. Tolerant of natural
/// input: ids may be `#`-prefixed and several may be passed at once. Missing or
/// malformed tickets report to stderr without suppressing the ones that resolve.
fn run_show(store: &Store, ids: &[String]) -> ExitCode {
    let mut ok = true;
    let mut first = true;
    for id in ids {
        match parse_id(id).and_then(|n| store.show(n).map_err(|e| e.to_string())) {
            Ok(s) => {
                if !first {
                    println!();
                }
                print!("{s}");
                first = false;
            }
            Err(e) => {
                eprintln!("pst: {e}");
                ok = false;
            }
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Parse a ticket id, tolerating a leading `#` and surrounding whitespace so
/// `pst show '#25'` works as naturally as `pst show 25`.
fn parse_id(raw: &str) -> Result<usize, String> {
    let t = raw.trim();
    let digits = t.strip_prefix('#').map_or(t, str::trim_start);
    digits
        .parse::<usize>()
        .map_err(|_| format!("invalid ticket id '{raw}'; expected a number like 25 or #25"))
}

fn parse_status(word: &str) -> Result<Status, String> {
    Status::from_word(word).ok_or_else(|| format!("invalid status '{word}' (open|wip|closed)"))
}

/// `--tag` allows hyphen values so `-x` removals parse, which also lets it swallow
/// a following long flag; reject flag-like values so the mistake fails loudly.
fn tag_ops(raw: &[String]) -> Result<Vec<TagOp>, String> {
    raw.iter()
        .map(|t| match t.as_bytes() {
            [b'-', b'-', ..] => Err(format!("tag '{t}' looks like a flag; use --tag={t}")),
            [b'-', ..] => Ok(TagOp::Remove(t[1..].to_string())),
            [b'+', ..] => Ok(TagOp::Add(t[1..].to_string())),
            _ => Ok(TagOp::Add(t.clone())),
        })
        .collect()
}

fn run_lint(path: &Path) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("pst: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let errors = lint(&bytes);
    for e in &errors {
        match e {
            LintError::MissingTrailingNewline => eprintln!("pst: missing trailing newline"),
            LintError::Line { number, error } => eprintln!("pst: line {number}: {error:?}"),
        }
    }
    if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
