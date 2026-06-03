use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::{Value, json};

const MANDATE: &str = include_str!("../pst-mandate.md");
const SKILL: &str = include_str!("../skills/pst/SKILL.md");
const GIT_HOOK: &str = include_str!("../.pst-hooks/pre-commit");
const GIT_HOOK_MARK: &str = "pst pre-commit hook";

// Frozen on-disk contract: change these and existing repos grow a second block on next re-run.
const MD_S: &str = "<!-- pst-mandate:start -->";
const MD_E: &str = "<!-- pst-mandate:end -->";
const TOML_S: &str = "# pst-mandate:start";
const TOML_E: &str = "# pst-mandate:end";

const CLAUDE_HOOK: &str = "#!/bin/sh\n[ -f .pst/mandate.md ] && cat .pst/mandate.md\nexit 0\n";
const CLAUDE_HOOK_PATH: &str = ".claude/hooks/pst-mandate.sh";
const CLAUDE_SETTINGS: &str = ".claude/settings.json";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool { Cursor, Antigravity, Claude, Codex, Copilot }

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Cursor, Tool::Antigravity, Tool::Claude, Tool::Codex, Tool::Copilot];
    pub fn flag(self) -> &'static str {
        ["--cursor", "--antigravity", "--claude", "--codex", "--copilot"][self as usize]
    }
    fn detected(self) -> bool {
        match self {
            Tool::Cursor => is_dir(".cursor"),
            Tool::Antigravity => is_dir(".agents") || is_dir(".agent"),
            Tool::Claude => is_dir(".claude") || is_file("CLAUDE.md"),
            Tool::Codex => is_dir(".codex") || is_file("AGENTS.md"),
            Tool::Copilot => is_dir(".github"),
        }
    }
}

#[derive(Default)]
pub struct Opts {
    pub tools: Vec<Tool>,
    pub show: bool,
    pub uninstall: bool,
}

pub fn init(opts: &Opts) -> io::Result<ExitCode> {
    if opts.show { return Ok(show()); }
    if opts.uninstall { return uninstall().map(|()| ExitCode::SUCCESS); }

    scaffold()?;
    install_git_hook()?;

    let targets: Vec<Tool> = if opts.tools.is_empty() {
        Tool::ALL.into_iter().filter(|t| t.detected()).collect()
    } else { opts.tools.clone() };

    if targets.is_empty() {
        let flags = Tool::ALL.iter().map(|t| t.flag()).collect::<Vec<_>>().join(" ");
        eprintln!("error: no agent config detected in this repo.");
        eprintln!("       pass one of: {flags}");
        eprintln!("       or create CLAUDE.md / AGENTS.md first.");
        return Ok(ExitCode::from(2));
    }

    for t in targets {
        match t {
            Tool::Cursor => write_cursor()?,
            Tool::Antigravity => write_antigravity()?,
            Tool::Claude => write_claude()?,
            Tool::Codex => write_codex()?,
            Tool::Copilot => write_copilot()?,
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn scaffold() -> io::Result<()> {
    fs::create_dir_all(".pst/details")?;
    write_if_missing(".pst/tickets", "")?; // never clobber the user's ticket DB
    // mandate/skill are verbatim copies of the embedded docs — refresh them on
    // re-run so `brew upgrade` + re-`init` propagates new guidance to old repos.
    write_synced(".pst/mandate.md", MANDATE)?;
    write_synced(".pst/skill.md", SKILL)
}

fn write_if_missing(path: &str, content: &str) -> io::Result<()> {
    let p = Path::new(path);
    if p.exists() { println!("ok      {path} (exists)"); }
    else { fs::write(p, content)?; println!("created {path}"); }
    Ok(())
}

/// Write a pst-owned data file, refreshing it in place when the embedded content
/// has changed (so upgrades reach existing repos) and no-op'ing when it already
/// matches. Unlike `write_if_missing`, an existing file is overwritten.
fn write_synced(path: &str, content: &str) -> io::Result<()> {
    match fs::read_to_string(path) {
        Ok(c) if c == content => println!("ok      {path} (current)"),
        Ok(_) => { fs::write(path, content)?; println!("updated {path}"); }
        Err(_) => { fs::write(path, content)?; println!("created {path}"); }
    }
    Ok(())
}

fn install_git_hook() -> io::Result<()> {
    let Some(dir) = git_hooks_dir() else {
        println!("skip    pre-commit hook (not a git repo)");
        return Ok(());
    };
    let f = dir.join("pre-commit");
    match fs::read_to_string(&f) {
        Ok(c) if c == GIT_HOOK => { println!("ok      {} (current)", f.display()); return Ok(()); }
        Ok(c) if !c.contains(GIT_HOOK_MARK) => { println!("skip    {} (foreign hook left untouched)", f.display()); return Ok(()); }
        _ => {}
    }
    fs::create_dir_all(&dir)?;
    fs::write(&f, GIT_HOOK)?;
    set_exec(&f)?;
    println!("hook    {}", f.display());
    Ok(())
}

fn git_hooks_dir() -> Option<PathBuf> {
    let out = Command::new("git").args(["rev-parse", "--git-path", "hooks"]).output().ok()?;
    let p = out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())?;
    (!p.is_empty()).then(|| PathBuf::from(p))
}

#[cfg(unix)]
fn set_exec(p: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o755))
}
#[cfg(not(unix))]
fn set_exec(_: &Path) -> io::Result<()> { Ok(()) }

fn write_cursor() -> io::Result<()> {
    let body = format!("---\ndescription: pst work-tracking mandate\nalwaysApply: true\n---\n\n{}\n", MANDATE.trim_end());
    write_owned(".cursor/rules/pst-mandate.mdc", &body)
}

fn write_antigravity() -> io::Result<()> {
    // Prefer legacy `.agent/` only when current `.agents/` is absent.
    let dir = if is_dir(".agent") && !is_dir(".agents") { ".agent" } else { ".agents" };
    let path = format!("{dir}/rules/pst-mandate.md");
    let body = format!("---\ndescription: pst work-tracking mandate\nactivation: always\n---\n\n{}\n", MANDATE.trim_end());
    write_owned(&path, &body)
}

fn write_copilot() -> io::Result<()> {
    let path = ".github/copilot-instructions.md";
    let verb = upsert_marked(path, MD_S, MD_E, MANDATE.trim_end())?;
    println!("{verb:7} {path}");
    Ok(())
}

fn write_codex() -> io::Result<()> {
    if MANDATE.contains("'''") {
        return Err(io::Error::other("mandate contains ''' which conflicts with TOML literal triple-string"));
    }
    let path = ".codex/config.toml";
    if has_unowned_developer_instructions(&fs::read_to_string(path).unwrap_or_default()) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,
            format!("{path} already defines developer_instructions outside pst markers; remove it or skip --codex")));
    }
    let body = format!("developer_instructions = '''\n{}\n'''", MANDATE.trim_end());
    let verb = upsert_marked(path, TOML_S, TOML_E, &body)?;
    println!("{verb:7} {path}");
    Ok(())
}

fn has_unowned_developer_instructions(s: &str) -> bool {
    let stripped = match (s.find(TOML_S), s.find(TOML_E)) {
        (Some(a), Some(b)) if b > a => format!("{}{}", &s[..a], &s[b + TOML_E.len()..]),
        _ => s.to_owned(),
    };
    stripped.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with('#') && t.starts_with("developer_instructions")
    })
}

fn write_claude() -> io::Result<()> {
    fs::create_dir_all(".claude/hooks")?;
    let prev = fs::read_to_string(CLAUDE_HOOK_PATH).ok();
    if prev.as_deref() == Some(CLAUDE_HOOK) {
        println!("ok      {CLAUDE_HOOK_PATH} (current)");
    } else {
        fs::write(CLAUDE_HOOK_PATH, CLAUDE_HOOK)?;
        set_exec(Path::new(CLAUDE_HOOK_PATH))?;
        println!("hook    {CLAUDE_HOOK_PATH}");
    }
    let verb = upsert_claude_settings()?;
    println!("{verb:7} {CLAUDE_SETTINGS} (SessionStart hook entry)");
    Ok(())
}

fn claude_entry() -> Value {
    json!({
        "matcher": "startup|resume|clear|compact",
        "hooks": [{ "type": "command", "command": "./.claude/hooks/pst-mandate.sh" }]
    })
}

// We own only SessionStart entries whose inner command ends with our script path.
fn is_owned(entry: &Value) -> bool {
    entry.get("hooks").and_then(Value::as_array).is_some_and(|hs|
        hs.iter().any(|h| h.get("command").and_then(Value::as_str).is_some_and(|s| s.ends_with("pst-mandate.sh"))))
}

fn upsert_claude_settings() -> io::Result<&'static str> {
    fs::create_dir_all(".claude")?;
    let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(CLAUDE_SETTINGS)?;
    f.lock()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;

    let mut root: Value = if buf.trim().is_empty() { json!({}) }
        else { serde_json::from_str(&buf).map_err(|e| invalid(
            format!("{CLAUDE_SETTINGS} is not valid JSON ({e}); refusing to overwrite. Fix the file and retry.")))? };

    let obj = root.as_object_mut().ok_or_else(|| invalid(format!("{CLAUDE_SETTINGS} root is not an object")))?;
    let hooks = obj.entry("hooks").or_insert_with(|| json!({})).as_object_mut()
        .ok_or_else(|| invalid(format!("{CLAUDE_SETTINGS}: `hooks` is not an object")))?;
    let arr = hooks.entry("SessionStart").or_insert_with(|| json!([])).as_array_mut()
        .ok_or_else(|| invalid(format!("{CLAUDE_SETTINGS}: `hooks.SessionStart` is not an array")))?;

    let entry = claude_entry();
    let verb = match arr.iter().position(is_owned) {
        Some(i) if arr[i] == entry => "ok",
        Some(i) => { arr[i] = entry; "updated" }
        None => { arr.push(entry); "added" }
    };
    if verb != "ok" {
        let pretty = serde_json::to_string_pretty(&root)? + "\n";
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(pretty.as_bytes())?;
    }
    Ok(verb)
}

fn write_owned(path: &str, content: &str) -> io::Result<()> {
    if let Some(p) = Path::new(path).parent() { fs::create_dir_all(p)?; }
    if fs::read_to_string(path).is_ok_and(|c| c == content) { println!("ok      {path} (current)"); }
    else { fs::write(path, content)?; println!("rule    {path}"); }
    Ok(())
}

fn upsert_marked(path: &str, ms: &str, me: &str, body: &str) -> io::Result<&'static str> {
    let cur = fs::read_to_string(path).unwrap_or_default();
    let block = format!("{ms}\n{body}\n{me}");
    let (next, verb) = match (cur.find(ms), cur.find(me)) {
        (Some(s), Some(e)) if e > s => {
            let new = format!("{}{block}{}", &cur[..s], &cur[e + me.len()..]);
            if new == cur { return Ok("ok"); }
            (new, "updated")
        }
        _ if cur.is_empty() => (format!("{block}\n"), "created"),
        _ => {
            let lead = if cur.ends_with('\n') { "\n" } else { "\n\n" };
            (format!("{cur}{lead}{block}\n"), "appended")
        }
    };
    if let Some(p) = Path::new(path).parent() { fs::create_dir_all(p)?; }
    fs::write(path, next)?;
    Ok(verb)
}

fn show() -> ExitCode {
    let mut any = false;
    any |= present(".pst/tickets", "ticket DB");
    any |= synced(".pst/mandate.md", MANDATE, "mandate");
    any |= synced(".pst/skill.md", SKILL, "skill");
    if let Some(d) = git_hooks_dir() {
        let f = d.join("pre-commit");
        match fs::read_to_string(&f) {
            Ok(c) if c == GIT_HOOK => println!("ok      {:40} (pst lint hook)", f.display()),
            Ok(c) if c.contains(GIT_HOOK_MARK) => println!("stale   {:40} (pst hook, content drift)", f.display()),
            Ok(_) => println!("foreign {:40} (left untouched)", f.display()),
            Err(_) => {}
        }
    }
    for (path, label) in [
        (".cursor/rules/pst-mandate.mdc", "Cursor rule"),
        (".agents/rules/pst-mandate.md", "Antigravity rule"),
        (".agent/rules/pst-mandate.md", "Antigravity rule (legacy)"),
        (CLAUDE_HOOK_PATH, "Claude hook script"),
    ] { any |= present(path, label); }
    if is_file(CLAUDE_SETTINGS) {
        match claude_settings_state() {
            Some(true) => { println!("ok      {CLAUDE_SETTINGS:40} (pst SessionStart entry present)"); any = true; }
            Some(false) => println!("missing {CLAUDE_SETTINGS:40} (no pst SessionStart entry)"),
            None => println!("error   {CLAUDE_SETTINGS:40} (invalid JSON)"),
        }
    }
    for (path, mark, label) in [
        (".codex/config.toml", TOML_S, "Codex developer_instructions"),
        (".github/copilot-instructions.md", MD_S, "Copilot instructions"),
    ] {
        if let Ok(s) = fs::read_to_string(path) {
            let has_block = s.contains(mark);
            println!("{:7} {path:40} ({label}, {})",
                if has_block { "ok" } else { "missing" },
                if has_block { "pst block present" } else { "no pst block" });
            any |= has_block;
        }
    }
    if !any { println!("(no pst state in this directory)"); }
    ExitCode::SUCCESS
}

/// Report a present-or-absent pst file; returns whether it exists.
fn present(path: &str, label: &str) -> bool {
    let ok = is_file(path);
    if ok { println!("ok      {path:40} ({label})"); }
    ok
}

/// Report a managed doc, flagging content drift from the embedded copy so a
/// stale `.pst/mandate.md`/`.pst/skill.md` is visible. Returns whether present.
fn synced(path: &str, embedded: &str, label: &str) -> bool {
    match fs::read_to_string(path) {
        Ok(c) if c == embedded => println!("ok      {path:40} ({label})"),
        Ok(_) => println!("stale   {path:40} ({label}, content drift — re-run pst init)"),
        Err(_) => return false,
    }
    true
}

fn claude_settings_state() -> Option<bool> {
    let s = fs::read_to_string(CLAUDE_SETTINGS).ok()?;
    if s.trim().is_empty() { return Some(false); }
    let v: Value = serde_json::from_str(&s).ok()?;
    let arr = v.get("hooks")?.get("SessionStart")?.as_array()?;
    Some(arr.iter().any(is_owned))
}

fn uninstall() -> io::Result<()> {
    for p in [".pst/mandate.md", ".pst/skill.md", ".cursor/rules/pst-mandate.mdc",
              ".agents/rules/pst-mandate.md", ".agent/rules/pst-mandate.md", CLAUDE_HOOK_PATH] {
        remove_file_owned(p)?;
    }
    if let Some(d) = git_hooks_dir() {
        let f = d.join("pre-commit");
        if let Ok(c) = fs::read_to_string(&f)
            && (c == GIT_HOOK || c.contains(GIT_HOOK_MARK))
        {
            fs::remove_file(&f)?;
            println!("removed {}", f.display());
        }
    }
    for d in [".cursor/rules", ".agents/rules", ".agent/rules", ".claude/hooks"] {
        let _ = fs::remove_dir(d); // succeeds iff empty
    }
    if is_file(CLAUDE_SETTINGS) { prune_claude_settings()?; }
    if is_file(".codex/config.toml") {
        strip_marker_block(".codex/config.toml", TOML_S, TOML_E)?;
        let _ = fs::remove_dir(".codex");
    }
    if is_file(".github/copilot-instructions.md") {
        strip_marker_block(".github/copilot-instructions.md", MD_S, MD_E)?;
    }
    Ok(())
}

fn remove_file_owned(path: &str) -> io::Result<()> {
    if is_file(path) { fs::remove_file(path)?; println!("removed {path}"); }
    Ok(())
}

fn strip_marker_block(path: &str, start: &str, end: &str) -> io::Result<()> {
    let cur = fs::read_to_string(path)?;
    let (Some(a), Some(b)) = (cur.find(start), cur.find(end)) else { return Ok(()); };
    if b <= a { return Ok(()); }
    let head = cur[..a].trim_end_matches('\n');
    let tail = cur[b + end.len()..].trim_start_matches('\n');
    let mut out = String::with_capacity(head.len() + tail.len() + 2);
    out.push_str(head);
    if !head.is_empty() && !tail.is_empty() { out.push_str("\n\n"); }
    else if !head.is_empty() || !tail.is_empty() { out.push('\n'); }
    out.push_str(tail);
    if out.trim().is_empty() { fs::remove_file(path)?; println!("removed {path}"); }
    else { fs::write(path, out)?; println!("updated {path} (pst block stripped)"); }
    Ok(())
}

fn prune_claude_settings() -> io::Result<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(CLAUDE_SETTINGS)?;
    f.lock()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    if buf.trim().is_empty() { return Ok(()); }
    let mut root: Value = serde_json::from_str(&buf)
        .map_err(|e| invalid(format!("{CLAUDE_SETTINGS} is not valid JSON: {e}")))?;
    let Some(obj) = root.as_object_mut() else { return Ok(()); };
    let mut changed = false;
    if let Some(hooks) = obj.get_mut("hooks").and_then(Value::as_object_mut) {
        if let Some(arr) = hooks.get_mut("SessionStart").and_then(Value::as_array_mut) {
            let before = arr.len();
            arr.retain(|e| !is_owned(e));
            changed = arr.len() != before;
            if arr.is_empty() { hooks.remove("SessionStart"); }
        }
        if hooks.is_empty() { obj.remove("hooks"); }
    }
    if !changed { return Ok(()); }
    if obj.is_empty() {
        drop(f);
        fs::remove_file(CLAUDE_SETTINGS)?;
        println!("removed {CLAUDE_SETTINGS}");
    } else {
        let pretty = serde_json::to_string_pretty(&root)? + "\n";
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(pretty.as_bytes())?;
        println!("updated {CLAUDE_SETTINGS} (pst entry removed)");
    }
    Ok(())
}

fn is_dir(p: &str) -> bool { Path::new(p).is_dir() }
fn is_file(p: &str) -> bool { Path::new(p).is_file() }
fn invalid(msg: String) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg) }
