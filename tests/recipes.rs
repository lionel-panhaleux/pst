use std::process::Command;

fn fixture() -> String {
    format!("{}/tests/fixtures/sample.tickets", env!("CARGO_MANIFEST_DIR"))
}

fn sh(cmd: &str) -> String {
    let out = Command::new("sh").arg("-c").arg(cmd).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn read_recipes() {
    let f = fixture();
    assert_eq!(sh(&format!("wc -l < '{f}'")).trim(), "3");
    assert!(sh(&format!("sed -n '2p' '{f}'")).contains("Second ticket about @bob"));
}

#[test]
fn grep_recipes() {
    let f = fixture();
    assert!(sh(&format!("grep -n '^open' '{f}'")).starts_with("1:"));
    assert_eq!(sh(&format!("grep -nc 'parent:#42' '{f}'")).trim(), "2");
    assert!(sh(&format!("grep -n '@alice' '{f}'")).starts_with("1:"));
}
