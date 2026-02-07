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
        .args(["a", archive.to_str().unwrap(), "src.txt", "-m1", "-t1"])
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
        .args(["a", archive.to_str().unwrap(), "src.bin", "-m2", "-t1"])
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

#[test]
fn rust_extracts_unmodeled_reference_archive() {
    ensure_ref_built();

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src_m0.txt");
    let archive = dir.path().join("m0.zpaq");
    let extracted_dir = dir.path().join("out");

    let payload = b"native rust decode of zpaq m0 path\nsecond line\n";
    fs::write(&src, payload).expect("write src");

    let status = StdCommand::new(ref_bin())
        .current_dir(dir.path())
        .args(["a", archive.to_str().unwrap(), "src_m0.txt", "-m0", "-t1"])
        .status()
        .expect("run zpaq add");
    assert!(status.success(), "zpaq add failed");

    let segs = zpars::extract_zpaq_unmodeled_file(&archive).expect("rust extract m0");
    assert!(
        segs.iter().any(|s| s.data.starts_with(payload)),
        "payload prefix not found in decoded segments"
    );

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "extract-zpaq-m0",
            "-i",
            archive.to_str().unwrap(),
            "-o",
            extracted_dir.to_str().unwrap(),
        ])
        .assert()
        .success();

    let mut found = false;
    for entry in fs::read_dir(&extracted_dir).expect("read extracted dir") {
        let path = entry.expect("entry").path();
        if path.is_file() {
            let bytes = fs::read(&path).expect("read extracted file");
            if bytes.starts_with(payload) {
                found = true;
                break;
            }
        }
    }
    assert!(found, "no extracted file starts with expected payload");
}

#[test]
fn auto_extract_falls_back_for_modeled_archives() {
    ensure_ref_built();

    let dir = tempdir().expect("tempdir");
    let src = dir.path().join("src_m1.txt");
    let archive = dir.path().join("m1.zpaq");
    let out = dir.path().join("out_m1");

    let payload = b"modeled archive fallback test\n";
    fs::write(&src, payload).expect("write src");

    let status = StdCommand::new(ref_bin())
        .current_dir(dir.path())
        .args(["a", archive.to_str().unwrap(), "src_m1.txt", "-m1", "-t1"])
        .status()
        .expect("run zpaq add");
    assert!(status.success(), "zpaq add failed");

    Command::new(assert_cmd::cargo::cargo_bin!("zpars"))
        .args([
            "extract-zpaq",
            "-i",
            archive.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--reference-bin",
            ref_bin().to_str().unwrap(),
        ])
        .assert()
        .success();

    let restored = fs::read(out.join("src_m1.txt")).expect("read restored");
    assert_eq!(restored, payload);
}
