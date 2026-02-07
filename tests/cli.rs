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

#[test]
fn cli_compress_directory_as_tar_stream() {
    let dir = tempdir().expect("tempdir");
    let input_dir = dir.path().join("docs");
    let input_file = input_dir.join("a.txt");
    let compressed = dir.path().join("docs.zps");
    let restore_dir = dir.path().join("restored_docs");

    fs::create_dir_all(&input_dir).expect("mkdir");
    fs::write(&input_file, b"hello directory compression").expect("write");

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "compress",
            "-i",
            input_dir.to_str().unwrap(),
            "-o",
            compressed.to_str().unwrap(),
            "--level",
            "2",
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "decompress",
            "-i",
            compressed.to_str().unwrap(),
            "-o",
            restore_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let restored = fs::read(restore_dir.join("a.txt"))
        .or_else(|_| fs::read(restore_dir.join("./a.txt")))
        .expect("read restored file");
    assert_eq!(restored, b"hello directory compression");
}
