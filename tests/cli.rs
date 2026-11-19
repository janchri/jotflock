use serde_json::Value;
use std::{path::Path, process::Command};

fn cli(profile: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_jotflock"))
        .args(["cli"])
        .args(args)
        .env("JOTFLOCK_DATA_DIR", profile)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["ok"], output.status.success(), "{reply}");
    (output.status.success(), reply)
}

fn run(profile: &Path, args: &[&str]) -> Value {
    let (success, reply) = cli(profile, args);
    assert!(success, "{args:?}: {reply}");
    reply["result"].clone()
}

#[test]
fn offline_cli_persists_edits_across_processes() {
    let profile = tempfile::tempdir().unwrap();
    let profile = profile.path();
    let board = run(profile, &["board", "new"]);
    let board = board["id"].as_str().unwrap();

    let frame = run(profile, &["object", "add", "frame", "0", "0", "Ideas"]);
    let frame = frame["id"].as_str().unwrap();
    let note = run(profile, &["object", "add", "sticky", "600", "600", "New"]);
    let note = note["id"].as_str().unwrap();
    run(profile, &["object", "move", note, "70", "80"]);
    run(profile, &["object", "text", note, "Two words\nand π"]);
    run(profile, &["object", "move", frame, "100", "50"]);

    let (success, _) = cli(profile, &["object", "resize", note, "NaN", "100"]);
    assert!(!success);
    let note = run(profile, &["object", "show", note]);
    assert_eq!(note["text"], "Two words\nand π");
    assert_eq!(note["x"], 170.0);
    assert_eq!(note["y"], 130.0);
    assert_eq!(note["width"], 232.0);
    assert_eq!(note["parent"], frame);

    run(profile, &["board", "close", board]);
    assert_eq!(run(profile, &["board", "show", board])["open"], false);
    run(profile, &["board", "open", board]);
    assert_eq!(run(profile, &["board", "show"])["id"], board);
}
