use anyhow::{bail, Context, Result};
use aws_sdk_s3::Client;
use clap::Parser;
use image::GenericImageView;
use rand::Rng;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Reads the EXIF orientation tag from an image file.
///
/// # Parameters
/// - `path` — path to the image file
///
/// # Returns
/// The EXIF orientation value (1-8), or 1 (normal) if no EXIF data is found.
pub fn read_exif_orientation(path: &Path) -> Result<u16> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open file for EXIF reading: {}", path.display()))?;
    let mut bufreader = std::io::BufReader::new(file);

    match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(exif) => {
            if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
                Ok(field.display_value().to_string().parse().unwrap_or(1))
            } else {
                Ok(1)
            }
        }
        Err(_) => Ok(1),
    }
}

/// Applies EXIF orientation correction to an image.
///
/// Transforms the image according to the EXIF orientation tag so that
/// the output image appears correctly oriented regardless of how it was stored.
///
/// # Parameters
/// - `img` — the source image
/// - `orientation` — EXIF orientation value (1-8)
///
/// # Returns
/// The correctly oriented `DynamicImage`.
pub fn apply_orientation(img: image::DynamicImage, orientation: u16) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate90(),
        6 => img.rotate90(),
        7 => img.flipv().rotate90(),
        8 => img.rotate270(),
        _ => img,
    }
}

#[derive(Parser)]
#[command(name = "photo-uploader", about = "Upload photos to AWS S3 with resizing")]
struct Cli {
    /// Path to the image file
    image: PathBuf,

    /// Subfolder in the S3 bucket
    folder: Option<String>,

    /// Path to config file (overrides default search paths)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Force overwrite existing photo on remote
    #[arg(short, long)]
    force: bool,
}

pub struct Config {
    access_key_id: String,
    secret_access_key: String,
    region: String,
    bucket: String,
    max_width: u32,
    max_height: u32,
    default_folder: String,
}

/// Parses an INI config file and returns a Config struct.
///
/// Reads the file at `path`, parses `[aws]` and `[defaults]` sections,
/// and extracts required fields. Missing `region` defaults to "us-east-1".
///
/// # Parameters
/// - `path` — path to the config.ini file
///
/// # Returns
/// A `Config` with AWS credentials, bucket settings, and image resize defaults.
pub fn load_config(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_lowercase();
            sections.entry(current_section.clone()).or_default();
        } else if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            sections
                .entry(current_section.clone())
                .or_default()
                .insert(key, value);
        }
    }

    let get = |section: &str, key: &str| -> Result<String> {
        sections
            .get(section)
            .and_then(|s| s.get(key))
            .cloned()
            .with_context(|| format!("Missing {}.{}", section, key))
    };

    Ok(Config {
        access_key_id: get("aws", "access_key_id")?,
        secret_access_key: get("aws", "secret_access_key")?,
        region: get("aws", "region").unwrap_or_else(|_| "us-east-1".to_string()),
        bucket: get("aws", "bucket")?,
        max_width: get("defaults", "max_width")?
            .parse()
            .context("Invalid max_width")?,
        max_height: get("defaults", "max_height")?
            .parse()
            .context("Invalid max_height")?,
        default_folder: get("defaults", "default_folder").unwrap_or_default(),
    })
}

/// Resizes an image to fit within the given dimensions, preserving aspect ratio.
/// If the image is already within bounds, it is returned unchanged.
///
/// # Parameters
/// - `img` — the source image to resize
/// - `max_w` — maximum width in pixels
/// - `max_h` — maximum height in pixels
///
/// # Returns
/// The resized `DynamicImage`.
pub fn resize_image(img: image::DynamicImage, max_w: u32, max_h: u32) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_w && h <= max_h {
        return img;
    }
    img.resize(max_w.max(max_h), max_w.max(max_h), image::imageops::FilterType::Lanczos3)
}

/// Generates a random alphanumeric postfix of the given length.
/// Characters are lowercase digits 0-9 and letters a-z.
///
/// # Parameters
/// - `len` — number of characters to generate
///
/// # Returns
/// A random `String` of the specified length.
pub fn random_postfix(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}

/// Returns the MIME content type string based on the file extension.
///
/// # Parameters
/// - `path` — file path or name (extension is extracted from the last `.` separator)
///
/// # Returns
/// A content type string such as "image/jpeg", "image/png", or "application/octet-stream".
pub fn content_type_for(path: &str) -> &str {
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Builds the S3 object key for an image upload.
///
/// When `force` is true or the object does not yet exist (`object_exists` is false),
/// the base key `folder/file_stem.ext` is returned. Otherwise, a random postfix is
/// appended to avoid overwriting the existing object.
///
/// # Parameters
/// - `folder` — S3 subfolder (empty string means bucket root)
/// - `file_stem` — file name without extension
/// - `ext` — file extension (e.g. "jpg", "png")
/// - `force` — if true, always return the base key (overwrite mode)
/// - `object_exists` — whether the base key already exists in S3
///
/// # Returns
/// The resolved S3 object key as a `String`.
pub fn build_key(folder: &str, file_stem: &str, ext: &str, force: bool, object_exists: bool) -> String {
    let base = {
        let filename = format!("{}.{}", file_stem, ext);
        if folder.is_empty() {
            filename
        } else {
            format!("{}/{}", folder.trim_end_matches('/'), filename)
        }
    };
    if force || !object_exists {
        base
    } else {
        let postfix = random_postfix(8);
        let filename = format!("{}_{}.{}", file_stem, postfix, ext);
        if folder.is_empty() {
            filename
        } else {
            format!("{}/{}", folder.trim_end_matches('/'), filename)
        }
    }
}

/// Returns a list of candidate paths where `config.ini` may be located.
///
/// Search order: OS-specific config dir, current directory, executable directory.
///
/// # Returns
/// A `Vec<PathBuf>` of candidate config file paths.
pub fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "linux")]
        paths.push(home.join(".config/aws-photo-uploader/config.ini"));

        #[cfg(target_os = "macos")]
        paths.push(home.join("Library/Application Support/aws-photo-uploader/config.ini"));

        #[cfg(target_os = "windows")]
        if let Some(appdata) = dirs::config_dir() {
            paths.push(appdata.join("aws-photo-uploader/config.ini"));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config.ini"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("config.ini"));
        }
    }

    paths
}

/// Finds the config file to use, either from an explicit path or by searching.
///
/// If `explicit` is `Some`, that path is used directly (must exist).
/// Otherwise, searches `config_search_paths()` and returns the first match.
///
/// # Parameters
/// - `explicit` — optional explicit config file path from CLI args
///
/// # Returns
/// The resolved `PathBuf` of the config file, or an error if not found.
pub fn find_config_file(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        bail!("Config file not found: {}", path.display());
    }

    for path in config_search_paths() {
        if path.exists() {
            return Ok(path);
        }
    }

    bail!("No config file found. Searched:\n{}", {
        let mut msg = String::new();
        for path in config_search_paths() {
            msg.push_str(&format!("  {}\n", path.display()));
        }
        msg
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.image.exists() {
        bail!("Image file not found: {}", cli.image.display());
    }

    let config_path = find_config_file(cli.config.as_deref())?;
    let config = load_config(&config_path)?;

    let orientation = read_exif_orientation(&cli.image)?;

    let img = image::open(&cli.image)
        .with_context(|| format!("Failed to open image: {}", cli.image.display()))?;

    let img = apply_orientation(img, orientation);
    let resized = resize_image(img, config.max_width, config.max_height);

    let mut buf = std::io::Cursor::new(Vec::new());
    let ext = cli
        .image
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    match ext.as_str() {
        "png" => resized.write_to(&mut buf, image::ImageFormat::Png),
        "webp" => resized.write_to(&mut buf, image::ImageFormat::WebP),
        "gif" => resized.write_to(&mut buf, image::ImageFormat::Gif),
        _ => resized.write_to(&mut buf, image::ImageFormat::Jpeg),
    }
    .context("Failed to encode image")?;

    let image_bytes = buf.into_inner();

    let cred = aws_sdk_s3::config::Credentials::new(
        &config.access_key_id,
        &config.secret_access_key,
        None,
        None,
        "photo-uploader",
    );

    let sdk_config = aws_config::from_env()
        .credentials_provider(cred)
        .region(aws_sdk_s3::config::Region::new(config.region))
        .load()
        .await;

    let client = Client::new(&sdk_config);

    let file_stem = cli
        .image
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("photo");

    let folder = cli
        .folder
        .as_deref()
        .unwrap_or(&config.default_folder);

    let object_exists = if cli.force {
        false
    } else {
        client
            .head_object()
            .bucket(&config.bucket)
            .key(build_key(folder, file_stem, &ext, true, false))
            .send()
            .await
            .is_ok()
    };
    let final_key = build_key(folder, file_stem, &ext, cli.force, object_exists);
    let content_type = content_type_for(&final_key);

    client
        .put_object()
        .bucket(&config.bucket)
        .key(&final_key)
        .body(image_bytes.into())
        .content_type(content_type)
        .send()
        .await
        .context("Failed to upload to S3")?;

    println!("s3://{}/{}", config.bucket, final_key);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_test_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::new(w, h))
    }

    fn write_config(contents: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    // ---- load_config tests ----

    #[test]
    fn test_load_config_valid() {
        let f = write_config(
            "[aws]\n\
             access_key_id = AKIA123\n\
             secret_access_key = secret456\n\
             region = eu-west-1\n\
             bucket = my-bucket\n\
             \n\
             [defaults]\n\
             max_width = 800\n\
             max_height = 600\n\
             default_folder = photos\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "AKIA123");
        assert_eq!(cfg.secret_access_key, "secret456");
        assert_eq!(cfg.region, "eu-west-1");
        assert_eq!(cfg.bucket, "my-bucket");
        assert_eq!(cfg.max_width, 800);
        assert_eq!(cfg.max_height, 600);
        assert_eq!(cfg.default_folder, "photos");
    }

    #[test]
    fn test_load_config_missing_region_defaults() {
        let f = write_config(
            "[aws]\n\
             access_key_id = AKIAX\n\
             secret_access_key = secretX\n\
             bucket = test-bucket\n\
             \n\
             [defaults]\n\
             max_width = 100\n\
             max_height = 200\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.region, "us-east-1");
        assert_eq!(cfg.default_folder, "");
    }

    #[test]
    fn test_load_config_missing_required_field() {
        let f = write_config(
            "[aws]\n\
             access_key_id = AKIAX\n\
             \n\
             [defaults]\n\
             max_width = 100\n\
             max_height = 200\n",
        );
        assert!(load_config(f.path()).is_err());
    }

    #[test]
    fn test_load_config_comments_and_blank_lines() {
        let f = write_config(
            "; this is a comment\n\
             # so is this\n\
             \n\
             [aws]\n\
             ; inline comment\n\
             access_key_id = KEY\n\
             secret_access_key = SEC\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "KEY");
        assert_eq!(cfg.max_width, 10);
    }

    #[test]
    fn test_load_config_case_insensitive() {
        let f = write_config(
            "[AWS]\n\
             Access_Key_Id = K\n\
             Secret_Access_Key = S\n\
             Bucket = B\n\
             \n\
             [DEFAULTS]\n\
             Max_Width = 10\n\
             Max_Height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "K");
        assert_eq!(cfg.max_width, 10);
    }

    #[test]
    fn test_load_config_invalid_number() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = not_a_number\n\
             max_height = 20\n",
        );
        assert!(load_config(f.path()).is_err());
    }

    #[test]
    fn test_load_config_file_not_found() {
        assert!(load_config(Path::new("/nonexistent/config.ini")).is_err());
    }

    // ---- resize_image tests ----

    #[test]
    fn test_resize_within_bounds() {
        let img = make_test_image(100, 80);
        let result = resize_image(img, 200, 200);
        assert_eq!(result.dimensions(), (100, 80));
    }

    #[test]
    fn test_resize_exact_bounds() {
        let img = make_test_image(200, 200);
        let result = resize_image(img, 200, 200);
        assert_eq!(result.dimensions(), (200, 200));
    }

    #[test]
    fn test_resize_exceeds_width() {
        let img = make_test_image(400, 100);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_exceeds_height() {
        let img = make_test_image(100, 400);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_exceeds_both() {
        let img = make_test_image(800, 600);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_aspect_ratio_preserved() {
        let img = make_test_image(1000, 500);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        let ratio = w as f64 / h as f64;
        assert!((ratio - 2.0).abs() < 0.1, "ratio was {ratio}");
    }

    // ---- random_postfix tests ----

    #[test]
    fn test_random_postfix_length() {
        let s = random_postfix(8);
        assert_eq!(s.len(), 8);
    }

    #[test]
    fn test_random_postfix_length_zero() {
        let s = random_postfix(0);
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_random_postfix_valid_chars() {
        let s = random_postfix(100);
        assert!(s.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
    }

    #[test]
    fn test_random_postfix_uniqueness() {
        let a = random_postfix(16);
        let b = random_postfix(16);
        assert_ne!(a, b);
    }

    // ---- content_type_for tests ----

    #[test]
    fn test_content_type_jpg() {
        assert_eq!(content_type_for("photo.jpg"), "image/jpeg");
    }

    #[test]
    fn test_content_type_jpeg() {
        assert_eq!(content_type_for("photo.jpeg"), "image/jpeg");
    }

    #[test]
    fn test_content_type_png() {
        assert_eq!(content_type_for("image.png"), "image/png");
    }

    #[test]
    fn test_content_type_webp() {
        assert_eq!(content_type_for("pic.webp"), "image/webp");
    }

    #[test]
    fn test_content_type_gif() {
        assert_eq!(content_type_for("anim.gif"), "image/gif");
    }

    #[test]
    fn test_content_type_unknown() {
        assert_eq!(content_type_for("file.xyz"), "application/octet-stream");
    }

    #[test]
    fn test_content_type_no_extension() {
        assert_eq!(content_type_for("noext"), "application/octet-stream");
    }

    #[test]
    fn test_content_type_case_insensitive() {
        assert_eq!(content_type_for("photo.JPG"), "image/jpeg");
        assert_eq!(content_type_for("photo.Png"), "image/png");
        assert_eq!(content_type_for("photo.GiF"), "image/gif");
    }

    #[test]
    fn test_content_type_path_with_dirs() {
        assert_eq!(content_type_for("/some/path/photo.jpg"), "image/jpeg");
    }

    // ---- build_key tests ----

    #[test]
    fn test_build_key_no_folder() {
        assert_eq!(build_key("", "photo", "jpg", true, false), "photo.jpg");
    }

    #[test]
    fn test_build_key_with_folder() {
        assert_eq!(build_key("photos", "photo", "jpg", true, false), "photos/photo.jpg");
    }

    #[test]
    fn test_build_key_nested_folder() {
        assert_eq!(
            build_key("2024/january", "photo", "png", true, false),
            "2024/january/photo.png"
        );
    }

    #[test]
    fn test_build_key_folder_trailing_slash() {
        assert_eq!(build_key("photos/", "photo", "jpg", true, false), "photos/photo.jpg");
    }

    // ---- find_config_file tests ----

    #[test]
    fn test_find_config_explicit_path() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let found = find_config_file(Some(f.path())).unwrap();
        assert_eq!(found, f.path().to_path_buf());
    }

    #[test]
    fn test_find_config_explicit_not_found() {
        assert!(find_config_file(Some(Path::new("/nonexistent/config.ini"))).is_err());
    }

    #[test]
    fn test_find_config_returns_search_paths() {
        let paths = config_search_paths();
        assert!(!paths.is_empty());
        for p in &paths {
            assert!(p.to_string_lossy().contains("config.ini"));
        }
    }

    // ---- build_key force/object_exists tests ----

    #[test]
    fn test_build_key_force_overwrites() {
        let key = build_key("photos", "photo", "jpg", true, true);
        assert_eq!(key, "photos/photo.jpg");
    }

    #[test]
    fn test_build_key_force_no_conflict() {
        let key = build_key("photos", "photo", "jpg", true, false);
        assert_eq!(key, "photos/photo.jpg");
    }

    #[test]
    fn test_build_key_no_force_no_conflict() {
        let key = build_key("photos", "photo", "jpg", false, false);
        assert_eq!(key, "photos/photo.jpg");
    }

    #[test]
    fn test_build_key_conflict_appends_postfix() {
        let key = build_key("photos", "photo", "jpg", false, true);
        assert_ne!(key, "photos/photo.jpg");
        assert!(key.starts_with("photos/photo_"));
        assert!(key.ends_with(".jpg"));
    }

    #[test]
    fn test_build_key_conflict_no_folder() {
        let key = build_key("", "photo", "png", false, true);
        assert_ne!(key, "photo.png");
        assert!(key.starts_with("photo_"));
        assert!(key.ends_with(".png"));
    }

    // ---- apply_orientation tests ----

    #[test]
    fn test_apply_orientation_normal() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 1);
        assert_eq!(result.dimensions(), (100, 200));
    }

    #[test]
    fn test_apply_orientation_flip_h() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 2);
        assert_eq!(result.dimensions(), (100, 200));
    }

    #[test]
    fn test_apply_orientation_rotate_180() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 3);
        assert_eq!(result.dimensions(), (100, 200));
    }

    #[test]
    fn test_apply_orientation_flip_v() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 4);
        assert_eq!(result.dimensions(), (100, 200));
    }

    #[test]
    fn test_apply_orientation_transpose() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 5);
        assert_eq!(result.dimensions(), (200, 100));
    }

    #[test]
    fn test_apply_orientation_rotate_90() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 6);
        assert_eq!(result.dimensions(), (200, 100));
    }

    #[test]
    fn test_apply_orientation_transverse() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 7);
        assert_eq!(result.dimensions(), (200, 100));
    }

    #[test]
    fn test_apply_orientation_rotate_270() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 8);
        assert_eq!(result.dimensions(), (200, 100));
    }

    #[test]
    fn test_apply_orientation_invalid_defaults_to_noop() {
        let img = make_test_image(100, 200);
        let result = apply_orientation(img, 99);
        assert_eq!(result.dimensions(), (100, 200));
    }

    // ---- read_exif_orientation tests ----

    #[test]
    fn test_read_exif_nonexistent_file() {
        assert!(read_exif_orientation(Path::new("/nonexistent/photo.jpg")).is_err());
    }

    #[test]
    fn test_read_exif_no_exif_returns_one() {
        let img = make_test_image(10, 10);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_exif.jpg");
        img.write_to(&mut std::io::BufWriter::new(
            std::fs::File::create(&path).unwrap(),
        ), image::ImageFormat::Jpeg)
        .unwrap();
        let orientation = read_exif_orientation(&path).unwrap();
        assert_eq!(orientation, 1);
    }
}
