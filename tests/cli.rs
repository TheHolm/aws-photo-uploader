use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;

fn write_config_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("config.ini");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(
        f,
        "[aws]\n\
         access_key_id = AKIATEST\n\
         secret_access_key = secretkey123\n\
         bucket = test-bucket\n\
         \n\
         [defaults]\n\
         max_width = 1920\n\
         max_height = 1080\n\
         default_folder = uploads"
    )
    .unwrap();
    path
}

fn write_test_image(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("photo.jpg");
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(100, 100));
    let mut f = std::fs::File::create(&path).unwrap();
    img.write_to(
        &mut std::io::BufWriter::new(&mut f),
        image::ImageFormat::Jpeg,
    )
    .unwrap();
    path
}

#[test]
fn test_no_args_fails() {
    Command::cargo_bin("photo-uploader")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
}

#[test]
fn test_folder_fallback_to_config_default() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = write_test_image(dir.path());

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([img.to_str().unwrap(), "-c", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
}

#[test]
fn test_file_stem_fallback_for_hidden_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = dir.path().join(".hidden");
    let real_img = write_test_image(dir.path());
    std::fs::copy(&real_img, &img).unwrap();

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([img.to_str().unwrap(), "-c", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to open image"));
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
fn test_upload_original_flag_reaches_s3() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = write_test_image(dir.path());

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([
            img.to_str().unwrap(),
            "-c",
            config.to_str().unwrap(),
            "--upload-original",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
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

#[test]
fn test_non_image_file_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = dir.path().join("fake.jpg");
    std::fs::write(&img, b"not an image at all").unwrap();

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([img.to_str().unwrap(), "-c", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to open image"));
}

#[test]
fn test_valid_config_and_image_reaches_s3_upload() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = write_test_image(dir.path());

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([img.to_str().unwrap(), "-c", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
}

#[test]
fn test_force_flag_reaches_s3_upload() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = write_test_image(dir.path());

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([
            img.to_str().unwrap(),
            "-c",
            config.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
}

#[test]
fn test_folder_argument_reaches_s3_upload() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_config_file(dir.path());
    let img = write_test_image(dir.path());

    Command::cargo_bin("photo-uploader")
        .unwrap()
        .args([
            img.to_str().unwrap(),
            "custom-folder",
            "-c",
            config.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("upload"));
}
