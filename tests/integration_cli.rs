use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn fixture_exists(name: &str) -> bool {
    fixture_path(name).exists()
}

fn copy_fixture_to(temp: &TempDir, fixture: &str, target_name: &str) -> PathBuf {
    let src = fixture_path(fixture);
    assert!(src.exists(), "missing fixture: {}", src.display());
    let dst = temp.path().join(target_name);
    fs::copy(&src, &dst).expect("failed to copy fixture");
    dst
}

fn run_cli(args: &[&str]) -> std::process::Output {
    let bin = env!("CARGO_BIN_EXE_marker-fixer");
    Command::new(bin)
        .env("MARKER_FIXER_SKIP_RUNTIME_TOOL_BOOTSTRAP", "1")
        .args(args)
        .output()
        .expect("failed to run marker-fixer")
}

fn extract_xmp(file_path: &Path) -> Option<String> {
    let bytes = fs::read(file_path).expect("failed to read file for xmp extraction");
    let marker = b"<x:xmpmeta";
    let start = bytes
        .windows(marker.len())
        .position(|window| window == marker)?;
    let suffix = &bytes[start..];
    let end_marker = b"</x:xmpmeta>";
    let end_rel = suffix
        .windows(end_marker.len())
        .position(|window| window == end_marker)?;
    let end = start + end_rel + end_marker.len();
    Some(String::from_utf8_lossy(&bytes[start..end]).to_string())
}

fn count_xmp_markers(file_path: &Path) -> usize {
    extract_xmp(file_path)
        .expect("xmp should exist")
        .matches("xmpDM:startTime=")
        .count()
}

fn corrupt_existing_xmp(file_path: &Path) {
    let mut data = fs::read(file_path).expect("failed to read file");
    let needle = b"</x:xmpmeta>";
    let replacement = b"</x:xmpmetX>";
    if let Some(start) = data
        .windows(needle.len())
        .position(|window| window == needle)
    {
        data[start..start + needle.len()].copy_from_slice(replacement);
    } else {
        panic!("expected existing xmp payload to corrupt");
    }
    fs::write(file_path, data).expect("failed to write corrupted xmp file");
}

#[test]
fn converts_obs_chapters_into_xmp_markers() {
    if !fixture_exists("direct obs example with markers.mp4") {
        eprintln!("Skipping: fixture not present");
        return;
    }
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");

    let output = run_cli(&[
        input.to_str().unwrap(),
        "--output-suffix",
        "_fixed",
    ]);

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output_file = temp.path().join("input_fixed.mp4");
    assert!(output_file.exists());
    assert!(
        extract_xmp(&output_file).is_some(),
        "expected xmp in output file"
    );
    assert_eq!(count_xmp_markers(&output_file), 12);
}

#[test]
fn merge_run_does_not_duplicate_existing_markers() {
    if !fixture_exists("direct obs example with markers.mp4") {
        eprintln!("Skipping: fixture not present");
        return;
    }
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");

    let first = run_cli(&[
        input.to_str().unwrap(),
        "--output-suffix",
        "_fixed",
    ]);
    assert!(first.status.success());

    let output_file = temp.path().join("input_fixed.mp4");
    let second = run_cli(&[output_file.to_str().unwrap()]);
    assert!(
        second.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let second_output = temp.path().join("input_fixed_fixed.mp4");
    assert!(second_output.exists());
    assert_eq!(count_xmp_markers(&second_output), 12);
}

#[test]
fn malformed_xmp_requires_force_flag() {
    if !fixture_exists("direct obs example with markers.mp4") {
        eprintln!("Skipping: fixture not present");
        return;
    }
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");

    let bootstrap = run_cli(&[input.to_str().unwrap()]);
    assert!(
        bootstrap.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&bootstrap.stdout),
        String::from_utf8_lossy(&bootstrap.stderr)
    );
    let output_file = temp.path().join("input_fixed.mp4");
    corrupt_existing_xmp(&output_file);

    let without_force = run_cli(&[output_file.to_str().unwrap()]);
    assert!(!without_force.status.success());

    let with_force = run_cli(&[output_file.to_str().unwrap(), "--force"]);
    assert!(
        with_force.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&with_force.stdout),
        String::from_utf8_lossy(&with_force.stderr)
    );
}

#[test]
fn processing_never_overwrites_original_input_file() {
    if !fixture_exists("direct obs example with markers.mp4") {
        eprintln!("Skipping: fixture not present");
        return;
    }
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");
    let before = fs::read(&input).expect("failed to read input before run");

    let output = run_cli(&[input.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read(&input).expect("failed to read input after run");
    assert_eq!(before, after, "original input should stay untouched");
    assert!(temp.path().join("input_fixed.mp4").exists());
}

#[test]
fn directory_mode_is_not_recursive() {
    if !fixture_exists("direct obs example with markers.mp4") {
        eprintln!("Skipping: fixture not present");
        return;
    }
    let temp = TempDir::new().expect("failed to create tempdir");
    let root_file = copy_fixture_to(&temp, "direct obs example with markers.mp4", "root.mp4");
    let nested_dir = temp.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("failed to create nested dir");
    let nested_file = nested_dir.join("nested.mp4");
    fs::copy(
        fixture_path("direct obs example with markers.mp4"),
        &nested_file,
    )
    .expect("failed to copy nested fixture");

    let output = run_cli(&[temp.path().to_str().unwrap(), "--dry-run"]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(root_file.to_str().unwrap()));
    assert!(!stdout.contains(nested_file.to_str().unwrap()));
}
