use photo_uploader::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_config(contents: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

fn minimal_config() -> &'static str {
    "[aws]\n\
     access_key_id = AKIA_TEST\n\
     secret_access_key = secret_test\n\
     bucket = test-bucket\n\
     \n\
     [defaults]\n\
     max_width = 1920\n\
     max_height = 1080\n\
     default_folder = photos\n"
}

#[test]
fn test_full_config_load() {
    let f = write_config(minimal_config());
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.access_key_id, "AKIA_TEST");
    assert_eq!(cfg.secret_access_key, "secret_test");
    assert_eq!(cfg.region, "us-east-1");
    assert_eq!(cfg.bucket, "test-bucket");
    assert_eq!(cfg.max_width, 1920);
    assert_eq!(cfg.max_height, 1080);
    assert_eq!(cfg.default_folder, "photos");
}

#[test]
fn test_config_optional_fields() {
    let cfg_content = "\
[aws]
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET
endpoint_url = https://minio.local:9000
storage_class = STANDARD_IA

[defaults]
max_width = 800
max_height = 600
default_folder = uploads
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.endpoint_url.as_deref(), Some("https://minio.local:9000"));
    assert_eq!(cfg.storage_class.as_deref(), Some("STANDARD_IA"));
}

#[test]
fn test_config_missing_required_errors() {
    let cfg_content = "\
[aws]
access_key_id = KEY

[defaults]
max_width = 800
max_height = 600
";
    let f = write_config(cfg_content);
    assert!(load_config(f.path()).is_err());
}

#[test]
fn test_config_invalid_dimensions_errors() {
    let cfg_content = "\
[aws]
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET

[defaults]
max_width = not_a_number
max_height = 600
";
    let f = write_config(cfg_content);
    assert!(load_config(f.path()).is_err());
}

#[test]
fn test_config_region_default() {
    let cfg_content = "\
[aws]
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET

[defaults]
max_width = 100
max_height = 100
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.region, "us-east-1");
}

#[test]
fn test_config_custom_region() {
    let cfg_content = "\
[aws]
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET
region = ap-southeast-1

[defaults]
max_width = 100
max_height = 100
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.region, "ap-southeast-1");
}

#[test]
fn test_config_empty_folder_default() {
    let cfg_content = "\
[aws]
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET

[defaults]
max_width = 100
max_height = 100
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.default_folder, "");
}

#[test]
fn test_find_config_explicit() {
    let f = write_config(minimal_config());
    let found = find_config_file(Some(f.path())).unwrap();
    assert_eq!(found, f.path());
}

#[test]
fn test_find_config_explicit_missing() {
    let result = find_config_file(Some(std::path::Path::new("/no/such/file.ini")));
    assert!(result.is_err());
}

#[test]
fn test_find_config_search_returns_valid() {
    if let Ok(path) = find_config_file(None) {
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with("config.ini"));
    }
}

#[test]
fn test_config_case_insensitive() {
    let cfg_content = "\
[AWS]
Access_Key_Id = KEY
Secret_Access_Key = SECRET
Bucket = BUCKET

[DEFAULTS]
Max_Width = 100
Max_Height = 200
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.access_key_id, "KEY");
    assert_eq!(cfg.max_width, 100);
}

#[test]
fn test_config_comments_ignored() {
    let cfg_content = "\
; This is a comment
# This is also a comment
[aws]
; inline comment
access_key_id = KEY
secret_access_key = SECRET
bucket = BUCKET

[defaults]
max_width = 100
max_height = 200
";
    let f = write_config(cfg_content);
    let cfg = load_config(f.path()).unwrap();
    assert_eq!(cfg.access_key_id, "KEY");
}
