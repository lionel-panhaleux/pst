use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const STATUS_WIDTH: usize = 6;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_pst")
}

fn tmp(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("pst-{tag}-{n}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn pst(db: &Path, args: &[&str]) -> Output {
    Command::new(bin()).args(args).arg("--file").arg(db).output().unwrap()
}

fn read(p: &Path) -> Vec<u8> {
    fs::read(p).unwrap()
}

#[test]
fn lint_pass_and_fail() {
    let db = tmp("lint").join("t");
    fs::write(&db, b"open  \t\thello\n").unwrap();
    assert!(pst(&db, &["lint"]).status.success());
    fs::write(&db, b"broken\n").unwrap();
    assert!(!pst(&db, &["lint"]).status.success());
}

#[test]
fn version_flag_reports_crate_version() {
    let out = Command::new(bin()).arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        concat!("pst ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn add_appends_and_numbers() {
    let db = tmp("add").join("t");
    assert_eq!(String::from_utf8_lossy(&pst(&db, &["add", "first"]).stdout).trim(), "1");
    let out = pst(&db, &["add", "second", "--tag", "bug", "--status", "wip"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
    assert_eq!(read(&db), b"open  \t\tfirst\nwip   \tbug\tsecond\n");
}

#[test]
fn add_does_not_fuse_onto_unterminated_line() {
    let db = tmp("fuse").join("t");
    fs::write(&db, b"open  \t\tone").unwrap(); // no trailing newline
    assert_eq!(String::from_utf8_lossy(&pst(&db, &["add", "two"]).stdout).trim(), "2");
    assert_eq!(read(&db), b"open  \t\tone\nopen  \t\ttwo\n");
    assert!(pst(&db, &["lint"]).status.success());
}

#[test]
fn status_edits_are_in_place() {
    let db = tmp("status").join("t");
    fs::write(&db, b"open  \tbug\tone\nopen  \t\ttwo\n").unwrap();
    let before = read(&db);
    assert!(pst(&db, &["close", "1"]).status.success());
    let after = read(&db);
    assert_eq!(before.len(), after.len());
    assert!((0..before.len()).all(|i| i < STATUS_WIDTH || before[i] == after[i]));
    assert_eq!(&after[..STATUS_WIDTH], b"closed");
    assert!(pst(&db, &["reopen", "1"]).status.success());
    assert!(pst(&db, &["set", "2", "--status", "wip"]).status.success()); // routes to fast path
    assert_eq!(read(&db), b"open  \tbug\tone\nwip   \t\ttwo\n");
}

#[test]
fn set_tags_add_remove_dedup() {
    let db = tmp("tags").join("t");
    fs::write(&db, b"open  \tbug\tone\n").unwrap();
    pst(&db, &["set", "1", "--tag", "+login", "--tag", "+bug"]);
    assert_eq!(read(&db), b"open  \tbug,login\tone\n");
    pst(&db, &["set", "1", "--tag", "-bug"]);
    assert_eq!(read(&db), b"open  \tlogin\tone\n");
}

#[test]
fn set_body_rewrites_only_target_line() {
    let db = tmp("body").join("t");
    fs::write(&db, b"open  \t\tone\nopen  \t\ttwo\nopen  \t\tthree\n").unwrap();
    assert!(pst(&db, &["set", "2", "--body", "TWO"]).status.success());
    assert_eq!(read(&db), b"open  \t\tone\nopen  \t\tTWO\nopen  \t\tthree\n");
}

#[test]
fn set_rejects_no_op_and_flag_like_tag() {
    let db = tmp("reject").join("t");
    fs::write(&db, b"open  \t\tone\n").unwrap();
    assert!(!pst(&db, &["set", "1"]).status.success()); // nothing to set
    assert!(!pst(&db, &["set", "1", "--tag", "--status", "closed"]).status.success());
    assert_eq!(read(&db), b"open  \t\tone\n"); // unchanged
}

#[test]
fn close_rejects_malformed_line() {
    let db = tmp("malformed").join("t");
    fs::write(&db, b"garbage\n").unwrap();
    assert!(!pst(&db, &["close", "1"]).status.success());
    assert_eq!(read(&db), b"garbage\n"); // not corrupted
}

#[test]
fn close_rejects_when_status_separator_is_not_tab() {
    // A 6-byte line prefix followed by a space (not TAB) must not be pwritten over —
    // it isn't shaped like a status field.
    let db = tmp("malformed-sep").join("t");
    fs::write(&db, b"open   not even close\n").unwrap();
    assert!(!pst(&db, &["close", "1"]).status.success());
    assert_eq!(read(&db), b"open   not even close\n");
}

#[test]
fn show_with_and_without_detail() {
    let dir = tmp("show");
    let db = dir.join("t");
    fs::write(&db, b"wip   \tbug,login\tFix it parent:#42\nopen  \t\tplain\n").unwrap();
    fs::create_dir_all(dir.join("details")).unwrap();
    fs::write(dir.join("details/1-fix.md"), b"context\n").unwrap();

    let s = String::from_utf8(pst(&db, &["show", "1"]).stdout).unwrap();
    assert!(s.contains("#1") && s.contains("wip") && s.contains("bug, login"));
    assert!(s.contains("Fix it parent:#42") && s.contains("context"));

    let s2 = String::from_utf8(pst(&db, &["show", "2"]).stdout).unwrap();
    assert!(s2.contains("plain") && !s2.contains("context"));
}

fn pst_init(dir: &Path, args: &[&str]) -> Output {
    Command::new(bin()).arg("init").args(args).current_dir(dir).output().unwrap()
}

fn git_init(dir: &Path) {
    assert!(Command::new("git").arg("init").arg("-q").current_dir(dir).status().unwrap().success());
}

#[test]
fn init_scaffolds_and_installs_git_hook() {
    let dir = tmp("scaffold");
    git_init(&dir);
    fs::create_dir_all(dir.join(".cursor")).unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    assert!(dir.join(".pst/tickets").exists());
    assert!(dir.join(".pst/details").is_dir());
    assert!(dir.join(".pst/mandate.md").is_file());
    assert!(dir.join(".pst/skill.md").is_file());
    assert!(dir.join(".git/hooks/pre-commit").exists());
}

#[test]
fn init_no_target_exits_2_and_does_not_clobber() {
    let dir = tmp("no-target");
    let out = pst_init(&dir, &[]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--cursor") && err.contains("--claude") && err.contains("--codex"));
    // .pst/ is still scaffolded — we want the data dir even without a target.
    assert!(dir.join(".pst/tickets").exists());
    // But no tool-native files anywhere.
    assert!(!dir.join("CLAUDE.md").exists());
    assert!(!dir.join("AGENTS.md").exists());
}

#[test]
fn init_auto_detects_existing_tool_dirs() {
    let dir = tmp("autodetect");
    fs::create_dir_all(dir.join(".cursor")).unwrap();
    fs::create_dir_all(dir.join(".github")).unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    assert!(dir.join(".cursor/rules/pst-mandate.mdc").is_file());
    assert!(dir.join(".github/copilot-instructions.md").is_file());
    // Not detected → not written.
    assert!(!dir.join(".codex").exists());
    assert!(!dir.join(".claude").exists());
}

#[test]
fn init_cursor_creates_dir_when_flag_forces_target() {
    let dir = tmp("cursor-flag");
    assert!(pst_init(&dir, &["--cursor"]).status.success());
    let f = dir.join(".cursor/rules/pst-mandate.mdc");
    assert!(f.is_file());
    let s = String::from_utf8(read(&f)).unwrap();
    assert!(s.starts_with("---\n") && s.contains("alwaysApply: true"));
    assert!(s.contains("Track all multi-step work as pst tickets"));
}

#[test]
fn init_antigravity_prefers_agents_over_legacy_agent() {
    let dir = tmp("antigravity-both");
    fs::create_dir_all(dir.join(".agent")).unwrap();
    fs::create_dir_all(dir.join(".agents")).unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    assert!(dir.join(".agents/rules/pst-mandate.md").is_file());
    assert!(!dir.join(".agent/rules/pst-mandate.md").exists());
}

#[test]
fn init_antigravity_uses_legacy_when_only_agent_present() {
    let dir = tmp("antigravity-legacy");
    fs::create_dir_all(dir.join(".agent")).unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    assert!(dir.join(".agent/rules/pst-mandate.md").is_file());
}

#[test]
fn init_claude_writes_hook_and_settings_entry() {
    let dir = tmp("claude-fresh");
    assert!(pst_init(&dir, &["--claude"]).status.success());
    let hook = dir.join(".claude/hooks/pst-mandate.sh");
    assert!(hook.is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&read(&dir.join(".claude/settings.json"))).unwrap();
    let arr = settings["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["hooks"][0]["command"].as_str().unwrap().ends_with("pst-mandate.sh"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&hook).unwrap().permissions().mode() & 0o111, 0o111);
    }
}

#[test]
fn init_claude_preserves_unrelated_settings_and_other_hooks() {
    let dir = tmp("claude-merge");
    fs::create_dir_all(dir.join(".claude")).unwrap();
    let settings_path = dir.join(".claude/settings.json");
    fs::write(&settings_path, r#"{
  "permissions": {"allow": ["Bash(ls)"]},
  "model": "claude-opus-4-7",
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo pre"}]}],
    "SessionStart": [
      {"matcher": "startup", "hooks": [{"type": "command", "command": "echo other"}]}
    ]
  }
}"#).unwrap();
    assert!(pst_init(&dir, &["--claude"]).status.success());
    let v: serde_json::Value = serde_json::from_slice(&read(&settings_path)).unwrap();
    // Untouched keys.
    assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
    assert_eq!(v["model"], "claude-opus-4-7");
    // Other hook event untouched.
    assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "echo pre");
    // SessionStart: the user's entry survives; ours is appended.
    let arr = v["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["hooks"][0]["command"], "echo other");
    assert!(arr[1]["hooks"][0]["command"].as_str().unwrap().ends_with("pst-mandate.sh"));
}

#[test]
fn init_claude_refuses_invalid_json() {
    let dir = tmp("claude-invalid");
    fs::create_dir_all(dir.join(".claude")).unwrap();
    let settings = dir.join(".claude/settings.json");
    fs::write(&settings, b"{not json").unwrap();
    let out = pst_init(&dir, &["--claude"]);
    assert!(!out.status.success());
    assert_eq!(read(&settings), b"{not json"); // unchanged
}

#[test]
fn init_claude_idempotent_rerun() {
    let dir = tmp("claude-idem");
    assert!(pst_init(&dir, &["--claude"]).status.success());
    let before = read(&dir.join(".claude/settings.json"));
    assert!(pst_init(&dir, &["--claude"]).status.success());
    assert_eq!(read(&dir.join(".claude/settings.json")), before);
}

#[test]
fn init_codex_writes_developer_instructions_block() {
    let dir = tmp("codex-fresh");
    assert!(pst_init(&dir, &["--codex"]).status.success());
    let s = String::from_utf8(read(&dir.join(".codex/config.toml"))).unwrap();
    assert!(s.contains("# pst-mandate:start"));
    assert!(s.contains("developer_instructions = '''"));
    assert!(s.contains("Track all multi-step work as pst tickets"));
    assert!(s.contains("# pst-mandate:end"));
}

#[test]
fn init_codex_refuses_existing_developer_instructions() {
    let dir = tmp("codex-conflict");
    fs::create_dir_all(dir.join(".codex")).unwrap();
    let cfg = dir.join(".codex/config.toml");
    fs::write(&cfg, b"model = \"o4\"\ndeveloper_instructions = \"my own\"\n").unwrap();
    let out = pst_init(&dir, &["--codex"]);
    assert!(!out.status.success());
    assert_eq!(read(&cfg), b"model = \"o4\"\ndeveloper_instructions = \"my own\"\n"); // unchanged
}

#[test]
fn init_codex_preserves_other_keys() {
    let dir = tmp("codex-other-keys");
    fs::create_dir_all(dir.join(".codex")).unwrap();
    fs::write(dir.join(".codex/config.toml"), b"model = \"o4\"\n").unwrap();
    assert!(pst_init(&dir, &["--codex"]).status.success());
    let s = String::from_utf8(read(&dir.join(".codex/config.toml"))).unwrap();
    assert!(s.contains("model = \"o4\""));
    assert!(s.contains("developer_instructions = '''"));
}

#[test]
fn init_copilot_marker_upserts_in_existing_file() {
    let dir = tmp("copilot-existing");
    fs::create_dir_all(dir.join(".github")).unwrap();
    let p = dir.join(".github/copilot-instructions.md");
    fs::write(&p, "# My rules\n\nuse 2-space indent.\n").unwrap();
    assert!(pst_init(&dir, &["--copilot"]).status.success());
    let s = String::from_utf8(read(&p)).unwrap();
    assert!(s.contains("# My rules"));
    assert!(s.contains("use 2-space indent."));
    assert!(s.contains("<!-- pst-mandate:start -->"));
    assert!(s.contains("Track all multi-step work as pst tickets"));
    // Re-running is a no-op.
    let before = read(&p);
    assert!(pst_init(&dir, &["--copilot"]).status.success());
    assert_eq!(read(&p), before);
}

#[test]
fn init_foreign_pre_commit_hook_left_untouched() {
    let dir = tmp("hook-foreign");
    git_init(&dir);
    fs::create_dir_all(dir.join(".cursor")).unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    fs::write(dir.join(".git/hooks/pre-commit"), "#!/bin/sh\necho mine\n").unwrap();
    assert!(pst_init(&dir, &[]).status.success());
    assert_eq!(read(&dir.join(".git/hooks/pre-commit")), b"#!/bin/sh\necho mine\n");
}

#[test]
fn init_show_prints_installed_state() {
    let dir = tmp("show");
    assert!(pst_init(&dir, &["--cursor", "--codex"]).status.success());
    let out = pst_init(&dir, &["--show"]);
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains(".pst/tickets"));
    assert!(s.contains(".cursor/rules/pst-mandate.mdc"));
    assert!(s.contains(".codex/config.toml"));
}

#[test]
fn init_uninstall_removes_only_owned_files() {
    let dir = tmp("uninstall");
    git_init(&dir);
    // Pre-existing user content we must NOT touch.
    fs::create_dir_all(dir.join(".github")).unwrap();
    let copilot = dir.join(".github/copilot-instructions.md");
    fs::write(&copilot, "# My rules\n\nuse 2-space indent.\n").unwrap();
    // Pre-existing settings.json with our entry + an unrelated entry.
    fs::create_dir_all(dir.join(".claude")).unwrap();
    let settings = dir.join(".claude/settings.json");
    fs::write(&settings, r#"{"model": "x", "hooks": {"SessionStart": [{"matcher":"startup","hooks":[{"type":"command","command":"echo other"}]}]}}"#).unwrap();

    assert!(pst_init(&dir, &["--cursor", "--claude", "--codex", "--copilot"]).status.success());
    // Sanity: things were written.
    assert!(dir.join(".cursor/rules/pst-mandate.mdc").is_file());
    assert!(dir.join(".claude/hooks/pst-mandate.sh").is_file());
    let pre_commit = dir.join(".git/hooks/pre-commit");
    assert!(pre_commit.is_file());
    // Add some user data — must survive uninstall.
    fs::write(dir.join(".pst/tickets"), b"open  \t\tkept\n").unwrap();

    assert!(pst_init(&dir, &["--uninstall"]).status.success());
    // Tool-native files we own → gone.
    assert!(!dir.join(".cursor/rules/pst-mandate.mdc").exists());
    assert!(!dir.join(".claude/hooks/pst-mandate.sh").exists());
    assert!(!dir.join(".codex/config.toml").exists()); // pure-pst file → deleted
    assert!(!pre_commit.exists());
    // settings.json: only the other entry remains.
    let v: serde_json::Value = serde_json::from_slice(&read(&settings)).unwrap();
    let arr = v["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["hooks"][0]["command"], "echo other");
    assert_eq!(v["model"], "x");
    // Copilot instructions: marker block stripped, user content preserved.
    let c = String::from_utf8(read(&copilot)).unwrap();
    assert!(c.contains("# My rules") && c.contains("use 2-space indent."));
    assert!(!c.contains("<!-- pst-mandate:"));
    // User data preserved.
    assert_eq!(read(&dir.join(".pst/tickets")), b"open  \t\tkept\n");
}

#[test]
fn concurrent_adds_stay_lint_clean() {
    let db = tmp("concurrent").join("t");
    fs::write(&db, b"").unwrap();
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let db = db.clone();
            std::thread::spawn(move || pst(&db, &["add", &format!("ticket {i}")]))
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(read(&db).iter().filter(|&&b| b == b'\n').count(), 20);
    assert!(pst(&db, &["lint"]).status.success());
}
