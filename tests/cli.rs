use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn cli_roundtrip_file() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.bin");
    let compressed = dir.path().join("out.zps");
    let restored = dir.path().join("restored.bin");

    let data = b"abcabcabcabcabcabc----rust-zpaq-port";
    fs::write(&input, data).expect("write input");

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "-v",
            "compress",
            "-i",
            input.to_str().unwrap(),
            "-o",
            compressed.to_str().unwrap(),
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "decompress",
            "-i",
            compressed.to_str().unwrap(),
            "-o",
            restored.to_str().unwrap(),
        ])
        .assert()
        .success();

    let out = fs::read(restored).expect("read restored");
    assert_eq!(out, data);
}

#[test]
fn cli_rejects_invalid_stream() {
    let dir = tempdir().expect("tempdir");
    let bad = dir.path().join("bad.bin");
    let restored = dir.path().join("restored.bin");
    fs::write(&bad, b"not-a-stream").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "decompress",
            "-i",
            bad.to_str().unwrap(),
            "-o",
            restored.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("bad magic"));
}
