use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_no_args_fails() {
    Command::cargo_bin("photo-uploader")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_missing_image_file() {
    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args(["/nonexistent/photo.jpg"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Image file not found"));
}

#[test]
fn test_help_flag() {
    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Upload photos to AWS S3"));
}

#[test]
fn test_invalid_config_path() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("photo.jpg");
    std::fs::write(&img, b"not a real image").unwrap();

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([img.to_str().unwrap(), "-c", "/nonexistent/config.ini"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Config file not found"));
}
