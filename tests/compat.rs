use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::tempdir;

fn ensure_ref_built() {
    let status = StdCommand::new("make")
        .arg("-s")
        .arg("zpaq")
        .current_dir("tmp/zpaq")
        .status()
        .expect("failed to run make");
    assert!(status.success(), "failed to build tmp/zpaq/zpaq");
}

fn ref_bin() -> PathBuf {
    std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join("zpaq")
        .join("zpaq")
}

#[test]
fn reference_archive_smoke() {
    ensure_ref_built();
    assert!(
        Path::new(&ref_bin()).exists(),
        "reference zpaq binary missing"
    );

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.txt");
    let out = dir.path().join("out.txt");
    let archive = dir.path().join("a.zpaq");

    fs::write(&src, b"zpaq reference smoke test\nline2\n").expect("write src");

    let status = StdCommand::new(ref_bin())
        .current_dir(dir.path())
        .args([
            "a",
            archive.to_str().unwrap(),
            "src.txt",
            "-m1",
            "-t1",
        ])
        .status()
        .expect("run zpaq add");
    assert!(status.success(), "zpaq add failed");

    let status = StdCommand::new(ref_bin())
        .current_dir(dir.path())
        .args([
            "x",
            archive.to_str().unwrap(),
            "src.txt",
            "-to",
            "out.txt",
            "-t1",
        ])
        .status()
        .expect("run zpaq extract");
    assert!(status.success(), "zpaq extract failed");

    let extracted = fs::read(out).expect("read extracted");
    let original = fs::read(src).expect("read original");
    assert_eq!(extracted, original);
}

#[test]
fn rust_inspector_reads_reference_blocks() {
    ensure_ref_built();

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src.bin");
    let archive = dir.path().join("b.zpaq");

    let mut payload = Vec::new();
    payload.extend_from_slice(b"abcdabcdabcd");
    payload.extend((0..4096).map(|x| (x % 251) as u8));
    fs::write(&src, payload).expect("write src");

    let status = StdCommand::new(ref_bin())
        .current_dir(dir.path())
        .args([
            "a",
            archive.to_str().unwrap(),
            "src.bin",
            "-m2",
            "-t1",
        ])
        .status()
        .expect("run zpaq add");
    assert!(status.success(), "zpaq add failed");

    let blocks = zpars::inspect_zpaq_file(&archive).expect("inspect archive");
    assert!(!blocks.is_empty(), "expected at least one block");
    assert!(blocks.iter().any(|b| b.level == 1 || b.level == 2));

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args(["inspect-zpaq", "-i", archive.to_str().unwrap()])
        .assert()
        .success();
}
