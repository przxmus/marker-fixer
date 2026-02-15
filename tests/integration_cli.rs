use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const ADOBE_XMP_UUID: [u8; 16] = [
    0xBE, 0x7A, 0xCF, 0xCB, 0x97, 0xA9, 0x42, 0xE8, 0x9C, 0x71, 0x99, 0x94, 0x91, 0xE3, 0xAF,
    0xAC,
];

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
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
        .args(args)
        .output()
        .expect("failed to run marker-fixer")
}

fn extract_xmp(file_path: &Path) -> Option<String> {
    let bytes = fs::read(file_path).expect("failed to read file for xmp extraction");
    let marker = b"<x:xmpmeta";
    let start = bytes.windows(marker.len()).position(|window| window == marker)?;
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

fn append_malformed_xmp_uuid(file_path: &Path) {
    let mut data = fs::read(file_path).expect("failed to read file");
    let payload = b"<x:xmpmeta><broken";
    let size = (8 + 16 + payload.len()) as u32;

    data.extend_from_slice(&size.to_be_bytes());
    data.extend_from_slice(b"uuid");
    data.extend_from_slice(&ADOBE_XMP_UUID);
    data.extend_from_slice(payload);

    fs::write(file_path, data).expect("failed to write malformed xmp test file");
}

#[test]
fn converts_obs_chapters_into_xmp_markers() {
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");

    let output = run_cli(&[
        input.to_str().unwrap(),
        "--in-place",
        "false",
        "--output-suffix",
        "_fixed",
    ]);

    assert!(output.status.success(), "stdout: {}\nstderr: {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));

    let output_file = temp.path().join("input_fixed.mp4");
    assert!(output_file.exists());
    assert!(extract_xmp(&output_file).is_some(), "expected xmp in output file");
    assert_eq!(count_xmp_markers(&output_file), 12);
}

#[test]
fn merge_run_does_not_duplicate_existing_markers() {
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");

    let first = run_cli(&[
        input.to_str().unwrap(),
        "--in-place",
        "false",
        "--output-suffix",
        "_fixed",
    ]);
    assert!(first.status.success());

    let output_file = temp.path().join("input_fixed.mp4");
    let second = run_cli(&[output_file.to_str().unwrap()]);
    assert!(second.status.success(), "stdout: {}\nstderr: {}", String::from_utf8_lossy(&second.stdout), String::from_utf8_lossy(&second.stderr));

    assert_eq!(count_xmp_markers(&output_file), 12);
}

#[test]
fn malformed_xmp_requires_force_flag() {
    let temp = TempDir::new().expect("failed to create tempdir");
    let input = copy_fixture_to(&temp, "direct obs example with markers.mp4", "input.mp4");
    append_malformed_xmp_uuid(&input);

    let without_force = run_cli(&[input.to_str().unwrap()]);
    assert!(!without_force.status.success());

    let with_force = run_cli(&[input.to_str().unwrap(), "--force"]);
    assert!(with_force.status.success(), "stdout: {}\nstderr: {}", String::from_utf8_lossy(&with_force.stdout), String::from_utf8_lossy(&with_force.stderr));
}

#[test]
fn directory_mode_is_not_recursive() {
    let temp = TempDir::new().expect("failed to create tempdir");
    let root_file = copy_fixture_to(&temp, "direct obs example with markers.mp4", "root.mp4");
    let nested_dir = temp.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("failed to create nested dir");
    let nested_file = nested_dir.join("nested.mp4");
    fs::copy(fixture_path("direct obs example with markers.mp4"), &nested_file)
        .expect("failed to copy nested fixture");

    let output = run_cli(&[temp.path().to_str().unwrap(), "--dry-run"]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(root_file.to_str().unwrap()));
    assert!(!stdout.contains(nested_file.to_str().unwrap()));
}
