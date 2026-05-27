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
    fs::write(&db, b"open  \x1e\x1ehello\n").unwrap();
    assert!(pst(&db, &["lint"]).status.success());
    fs::write(&db, b"broken\n").unwrap();
    assert!(!pst(&db, &["lint"]).status.success());
}

#[test]
fn add_appends_and_numbers() {
    let db = tmp("add").join("t");
    assert_eq!(String::from_utf8_lossy(&pst(&db, &["add", "first"]).stdout).trim(), "1");
    let out = pst(&db, &["add", "second", "--tag", "bug", "--status", "wip"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2");
    assert_eq!(read(&db), b"open  \x1e\x1efirst\nwip   \x1ebug\x1esecond\n");
}

#[test]
fn add_does_not_fuse_onto_unterminated_line() {
    let db = tmp("fuse").join("t");
    fs::write(&db, b"open  \x1e\x1eone").unwrap(); // no trailing newline
    assert_eq!(String::from_utf8_lossy(&pst(&db, &["add", "two"]).stdout).trim(), "2");
    assert_eq!(read(&db), b"open  \x1e\x1eone\nopen  \x1e\x1etwo\n");
    assert!(pst(&db, &["lint"]).status.success());
}

#[test]
fn status_edits_are_in_place() {
    let db = tmp("status").join("t");
    fs::write(&db, b"open  \x1ebug\x1eone\nopen  \x1e\x1etwo\n").unwrap();
    let before = read(&db);
    assert!(pst(&db, &["close", "1"]).status.success());
    let after = read(&db);
    assert_eq!(before.len(), after.len());
    assert!((0..before.len()).all(|i| i < STATUS_WIDTH || before[i] == after[i]));
    assert_eq!(&after[..STATUS_WIDTH], b"closed");
    assert!(pst(&db, &["reopen", "1"]).status.success());
    assert!(pst(&db, &["set", "2", "--status", "wip"]).status.success()); // routes to fast path
    assert_eq!(read(&db), b"open  \x1ebug\x1eone\nwip   \x1e\x1etwo\n");
}

#[test]
fn set_tags_add_remove_dedup() {
    let db = tmp("tags").join("t");
    fs::write(&db, b"open  \x1ebug\x1eone\n").unwrap();
    pst(&db, &["set", "1", "--tag", "+login", "--tag", "+bug"]);
    assert_eq!(read(&db), b"open  \x1ebug\x1flogin\x1eone\n");
    pst(&db, &["set", "1", "--tag", "-bug"]);
    assert_eq!(read(&db), b"open  \x1elogin\x1eone\n");
}

#[test]
fn set_body_rewrites_only_target_line() {
    let db = tmp("body").join("t");
    fs::write(&db, b"open  \x1e\x1eone\nopen  \x1e\x1etwo\nopen  \x1e\x1ethree\n").unwrap();
    assert!(pst(&db, &["set", "2", "--body", "TWO"]).status.success());
    assert_eq!(read(&db), b"open  \x1e\x1eone\nopen  \x1e\x1eTWO\nopen  \x1e\x1ethree\n");
}

#[test]
fn set_rejects_no_op_and_flag_like_tag() {
    let db = tmp("reject").join("t");
    fs::write(&db, b"open  \x1e\x1eone\n").unwrap();
    assert!(!pst(&db, &["set", "1"]).status.success()); // nothing to set
    assert!(!pst(&db, &["set", "1", "--tag", "--status", "closed"]).status.success());
    assert_eq!(read(&db), b"open  \x1e\x1eone\n"); // unchanged
}

#[test]
fn close_rejects_malformed_line() {
    let db = tmp("malformed").join("t");
    fs::write(&db, b"garbage\n").unwrap();
    assert!(!pst(&db, &["close", "1"]).status.success());
    assert_eq!(read(&db), b"garbage\n"); // not corrupted
}

#[test]
fn show_with_and_without_detail() {
    let dir = tmp("show");
    let db = dir.join("t");
    fs::write(&db, b"wip   \x1ebug\x1flogin\x1eFix it parent:#42\nopen  \x1e\x1eplain\n").unwrap();
    fs::create_dir_all(dir.join("details")).unwrap();
    fs::write(dir.join("details/1-fix.md"), b"context\n").unwrap();

    let s = String::from_utf8(pst(&db, &["show", "1"]).stdout).unwrap();
    assert!(s.contains("#1") && s.contains("wip") && s.contains("bug, login"));
    assert!(s.contains("Fix it parent:#42") && s.contains("context"));

    let s2 = String::from_utf8(pst(&db, &["show", "2"]).stdout).unwrap();
    assert!(s2.contains("plain") && !s2.contains("context"));
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
