use anyhow::{bail, Context, Result};
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
                Ok(field.value.get_uint(0).unwrap_or(1) as u16)
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

#[derive(Debug)]
pub struct Config {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub bucket: String,
    pub endpoint_url: Option<String>,
    pub storage_class: Option<String>,
    pub base_url: Option<String>,
    pub max_width: u32,
    pub max_height: u32,
    pub default_folder: String,
    pub upload_original: bool,
    pub strip_exif: bool,
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
        endpoint_url: get("aws", "endpoint_url").ok(),
        storage_class: get("aws", "storage_class").ok(),
        base_url: get("aws", "base_url").ok(),
        max_width: get("defaults", "max_width")?
            .parse()
            .context("Invalid max_width")?,
        max_height: get("defaults", "max_height")?
            .parse()
            .context("Invalid max_height")?,
        default_folder: get("defaults", "default_folder").unwrap_or_default(),
        upload_original: get("defaults", "upload_original")
            .unwrap_or_else(|_| "no".to_string())
            .eq_ignore_ascii_case("yes"),
        strip_exif: get("defaults", "strip_exif")
            .unwrap_or_else(|_| "yes".to_string())
            .eq_ignore_ascii_case("yes"),
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
    let target_w = max_w.max(1);
    let target_h = max_h.max(1);
    let resized = img.resize(target_w, target_h, image::imageops::FilterType::Lanczos3);
    let (rw, rh) = resized.dimensions();
    if rw == 0 || rh == 0 {
        resized.resize(1, 1, image::imageops::FilterType::Nearest)
    } else {
        resized
    }
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
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
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
pub fn build_key(
    folder: &str,
    file_stem: &str,
    ext: &str,
    force: bool,
    object_exists: bool,
) -> String {
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

/// The full image processing pipeline: reads EXIF, applies orientation, resizes,
/// and encodes the image to bytes.
///
/// # Parameters
/// - `image_path` — path to the source image
/// - `max_width` — maximum output width
/// - `max_height` — maximum output height
///
/// # Returns
/// A tuple of `(encoded_bytes, extension_string)`.
pub fn process_image(
    image_path: &Path,
    max_width: u32,
    max_height: u32,
) -> Result<(Vec<u8>, String, u32, u32)> {
    let orientation = read_exif_orientation(image_path)?;

    let img = image::open(image_path)
        .with_context(|| format!("Failed to open image: {}", image_path.display()))?;

    let img = apply_orientation(img, orientation);
    let resized = resize_image(img, max_width, max_height);
    let (w, h) = resized.dimensions();

    let ext = image_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let mut buf = std::io::Cursor::new(Vec::new());
    match ext.as_str() {
        "png" => resized.write_to(&mut buf, image::ImageFormat::Png),
        "webp" => resized.write_to(&mut buf, image::ImageFormat::WebP),
        "gif" => resized.write_to(&mut buf, image::ImageFormat::Gif),
        _ => resized.write_to(&mut buf, image::ImageFormat::Jpeg),
    }
    .context("Failed to encode image")?;

    Ok((buf.into_inner(), ext, w, h))
}

/// Strips EXIF data from a JPEG byte stream by removing APP1 markers.
/// Preserves the JPEG SOI marker and all non-EXIF segments.
///
/// # Parameters
/// - `jpeg_bytes` — raw JPEG file bytes
///
/// # Returns
/// JPEG bytes with EXIF data removed, or the original bytes if no EXIF found.
pub fn strip_exif_from_jpeg(jpeg_bytes: &[u8]) -> Vec<u8> {
    if jpeg_bytes.len() < 4 || jpeg_bytes[0] != 0xFF || jpeg_bytes[1] != 0xD8 {
        return jpeg_bytes.to_vec();
    }

    let mut out = Vec::with_capacity(jpeg_bytes.len());
    out.extend_from_slice(&jpeg_bytes[..2]);

    let mut pos = 2;
    while pos + 1 < jpeg_bytes.len() {
        if jpeg_bytes[pos] != 0xFF {
            break;
        }
        let marker = jpeg_bytes[pos + 1];
        if marker == 0xD9 || marker == 0xDA {
            out.extend_from_slice(&jpeg_bytes[pos..]);
            return out;
        }
        if marker == 0xE1 {
            pos += 2;
            if pos + 1 < jpeg_bytes.len() {
                let seg_len = u16::from_be_bytes([jpeg_bytes[pos], jpeg_bytes[pos + 1]]) as usize;
                pos += seg_len;
            }
            continue;
        }
        if pos + 3 < jpeg_bytes.len() {
            let seg_len = u16::from_be_bytes([jpeg_bytes[pos + 2], jpeg_bytes[pos + 3]]) as usize;
            out.extend_from_slice(&jpeg_bytes[pos..pos + 2 + seg_len]);
            pos += 2 + seg_len;
        } else {
            out.extend_from_slice(&jpeg_bytes[pos..]);
            return out;
        }
    }
    out
}

/// Reads original image bytes, optionally stripping EXIF data.
///
/// # Parameters
/// - `path` — path to the original image file
/// - `strip` — if true, strip EXIF data (only affects JPEG)
///
/// # Returns
/// The file bytes, with EXIF removed if requested and format is JPEG.
pub fn original_bytes(path: &Path, strip: bool) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read original file: {}", path.display()))?;
    if strip && is_jpeg(path) {
        Ok(strip_exif_from_jpeg(&bytes))
    } else {
        Ok(bytes)
    }
}

fn is_jpeg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false)
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
        assert!(cfg.endpoint_url.is_none());
        assert!(cfg.storage_class.is_none());
    }

    #[test]
    fn test_load_config_with_endpoint_and_storage_class() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             endpoint_url = https://minio.example.com\n\
             storage_class = GLACIER\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(
            cfg.endpoint_url.as_deref(),
            Some("https://minio.example.com")
        );
        assert_eq!(cfg.storage_class.as_deref(), Some("GLACIER"));
    }

    #[test]
    fn test_load_config_with_base_url() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             base_url = https://cdn.example.com/images\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(
            cfg.base_url.as_deref(),
            Some("https://cdn.example.com/images")
        );
    }

    #[test]
    fn test_load_config_base_url_absent() {
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
        let cfg = load_config(f.path()).unwrap();
        assert!(cfg.base_url.is_none());
    }

    #[test]
    fn test_load_config_invalid_max_height() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 100\n\
             max_height = not_a_number\n",
        );
        assert!(load_config(f.path()).is_err());
    }

    #[test]
    fn test_load_config_duplicate_keys_last_wins() {
        let f = write_config(
            "[aws]\n\
             access_key_id = FIRST\n\
             access_key_id = SECOND\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "SECOND");
    }

    #[test]
    fn test_load_config_key_before_section() {
        let f = write_config(
            "access_key_id = ORPHAN\n\
             [aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "K");
    }

    #[test]
    fn test_load_config_whitespace_around_equals() {
        let f = write_config(
            "[aws]\n\
             access_key_id   =   K  \n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "K");
    }

    #[test]
    fn test_load_config_empty_file() {
        let f = write_config("");
        let result = load_config(f.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Missing"), "unexpected error: {}", msg);
    }

    #[test]
    fn test_load_config_equals_inside_value() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             endpoint_url = http://localhost:4566/path?q=1\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(
            cfg.endpoint_url.as_deref(),
            Some("http://localhost:4566/path?q=1")
        );
    }

    #[test]
    fn test_load_config_line_without_equals() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             this_line_has_no_equals_sign\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "K");
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
    fn test_random_postfix_single_char() {
        let s = random_postfix(1);
        assert_eq!(s.len(), 1);
        assert!(
            s.chars().next().unwrap().is_ascii_digit()
                || s.chars().next().unwrap().is_ascii_lowercase()
        );
    }

    #[test]
    fn test_random_postfix_valid_chars() {
        let s = random_postfix(100);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
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

    #[test]
    fn test_content_type_empty_string() {
        assert_eq!(content_type_for(""), "application/octet-stream");
    }

    #[test]
    fn test_content_type_trailing_dot() {
        assert_eq!(content_type_for("photo."), "application/octet-stream");
    }

    #[test]
    fn test_content_type_double_extension() {
        assert_eq!(
            content_type_for("archive.tar.gz"),
            "application/octet-stream"
        );
    }

    // ---- build_key tests ----

    #[test]
    fn test_build_key_no_folder() {
        assert_eq!(build_key("", "photo", "jpg", true, false), "photo.jpg");
    }

    #[test]
    fn test_build_key_with_folder() {
        assert_eq!(
            build_key("photos", "photo", "jpg", true, false),
            "photos/photo.jpg"
        );
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
        assert_eq!(
            build_key("photos/", "photo", "jpg", true, false),
            "photos/photo.jpg"
        );
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
        img.write_to(
            &mut std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let orientation = read_exif_orientation(&path).unwrap();
        assert_eq!(orientation, 1);
    }

    #[test]
    fn test_read_exif_exif_without_orientation_returns_one() {
        let img = make_test_image(10, 10);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exif_no_orientation.jpg");
        let mut jpeg_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut jpeg_bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg = jpeg_bytes.into_inner();

        let field = exif::Field {
            tag: exif::Tag::DateTime,
            ifd_num: exif::In::PRIMARY,
            value: exif::Value::Ascii(vec![b"2024:01:15 12:00:00".to_vec()]),
        };
        let mut writer = exif::experimental::Writer::new();
        writer.push_field(&field);
        let mut exif_buf = std::io::Cursor::new(Vec::new());
        writer.write(&mut exif_buf, false).unwrap();
        let tiff_data = exif_buf.into_inner();

        let mut app1 = vec![0xFF, 0xE1];
        let exif_header = b"Exif\0\0";
        let segment_len = 2 + exif_header.len() + tiff_data.len();
        app1.extend_from_slice(&(segment_len as u16).to_be_bytes());
        app1.extend_from_slice(exif_header);
        app1.extend_from_slice(&tiff_data);
        jpeg.splice(2..2, app1);

        std::fs::write(&path, &jpeg).unwrap();
        let orientation = read_exif_orientation(&path).unwrap();
        assert_eq!(orientation, 1);
    }

    #[test]
    fn test_read_exif_corrupted_jpeg_returns_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupted.jpg");
        std::fs::write(&path, b"this is not a jpeg file at all").unwrap();
        let orientation = read_exif_orientation(&path).unwrap();
        assert_eq!(orientation, 1);
    }

    fn write_exif_jpeg(path: &Path, w: u32, h: u32, orientation: u16) {
        let img = make_test_image(w, h);
        let mut jpeg_bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut jpeg_bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let mut jpeg = jpeg_bytes.into_inner();

        let field = exif::Field {
            tag: exif::Tag::Orientation,
            ifd_num: exif::In::PRIMARY,
            value: exif::Value::Short(vec![orientation]),
        };
        let mut writer = exif::experimental::Writer::new();
        writer.push_field(&field);
        let mut exif_buf = std::io::Cursor::new(Vec::new());
        writer.write(&mut exif_buf, false).unwrap();
        let tiff_data = exif_buf.into_inner();

        let mut app1 = vec![0xFF, 0xE1];
        let exif_header = b"Exif\0\0";
        let segment_len = 2 + exif_header.len() + tiff_data.len();
        app1.extend_from_slice(&(segment_len as u16).to_be_bytes());
        app1.extend_from_slice(exif_header);
        app1.extend_from_slice(&tiff_data);

        jpeg.splice(2..2, app1);

        std::fs::write(path, &jpeg).unwrap();
    }

    #[test]
    fn test_read_exif_orientation_1() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o1.jpg");
        write_exif_jpeg(&path, 10, 10, 1);
        assert_eq!(read_exif_orientation(&path).unwrap(), 1);
    }

    #[test]
    fn test_read_exif_orientation_2() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o2.jpg");
        write_exif_jpeg(&path, 10, 10, 2);
        assert_eq!(read_exif_orientation(&path).unwrap(), 2);
    }

    #[test]
    fn test_read_exif_orientation_3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o3.jpg");
        write_exif_jpeg(&path, 10, 10, 3);
        assert_eq!(read_exif_orientation(&path).unwrap(), 3);
    }

    #[test]
    fn test_read_exif_orientation_4() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o4.jpg");
        write_exif_jpeg(&path, 10, 10, 4);
        assert_eq!(read_exif_orientation(&path).unwrap(), 4);
    }

    #[test]
    fn test_read_exif_orientation_5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o5.jpg");
        write_exif_jpeg(&path, 10, 10, 5);
        assert_eq!(read_exif_orientation(&path).unwrap(), 5);
    }

    #[test]
    fn test_read_exif_orientation_6() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o6.jpg");
        write_exif_jpeg(&path, 10, 10, 6);
        assert_eq!(read_exif_orientation(&path).unwrap(), 6);
    }

    #[test]
    fn test_read_exif_orientation_7() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o7.jpg");
        write_exif_jpeg(&path, 10, 10, 7);
        assert_eq!(read_exif_orientation(&path).unwrap(), 7);
    }

    #[test]
    fn test_read_exif_orientation_8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o8.jpg");
        write_exif_jpeg(&path, 10, 10, 8);
        assert_eq!(read_exif_orientation(&path).unwrap(), 8);
    }

    #[test]
    fn test_read_exif_orientation_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.jpg");
        write_exif_jpeg(&path, 20, 30, 6);
        let orientation = read_exif_orientation(&path).unwrap();
        assert_eq!(orientation, 6);
    }

    // ---- resize_image additional tests ----

    #[test]
    fn test_resize_asymmetric_bounds() {
        let img = make_test_image(1000, 500);
        let result = resize_image(img, 200, 100);
        let (w, h) = result.dimensions();
        assert!(w <= 200, "width {w} exceeded max 200");
        assert!(h <= 100, "height {h} exceeded max 100");
    }

    #[test]
    fn test_resize_asymmetric_tall_image_respects_height_limit() {
        let img = make_test_image(100, 5000);
        let result = resize_image(img, 2000, 100);
        let (w, h) = result.dimensions();
        assert!(
            h <= 100,
            "height {h} should be <= 100, was not constrained by max_h"
        );
        assert!(w <= 2000, "width {w} should be <= 2000");
    }

    #[test]
    fn test_resize_asymmetric_wide_image_respects_width_limit() {
        let img = make_test_image(5000, 100);
        let result = resize_image(img, 100, 2000);
        let (w, h) = result.dimensions();
        assert!(
            w <= 100,
            "width {w} should be <= 100, was not constrained by max_w"
        );
        assert!(h <= 2000, "height {h} should be <= 2000");
    }

    #[test]
    fn test_resize_zero_max_clamps_to_one() {
        let img = make_test_image(100, 100);
        let result = resize_image(img, 0, 0);
        let (w, h) = result.dimensions();
        assert!(w >= 1, "width should be at least 1");
        assert!(h >= 1, "height should be at least 1");
    }

    #[test]
    fn test_resize_zero_max_width() {
        let img = make_test_image(200, 100);
        let result = resize_image(img, 0, 200);
        let (w, h) = result.dimensions();
        assert!(w >= 1, "width should be at least 1");
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_zero_max_height() {
        let img = make_test_image(100, 200);
        let result = resize_image(img, 200, 0);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h >= 1, "height should be at least 1");
    }

    #[test]
    fn test_resize_small_wide_image() {
        let img = make_test_image(400, 50);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_small_tall_image() {
        let img = make_test_image(50, 400);
        let result = resize_image(img, 200, 200);
        let (w, h) = result.dimensions();
        assert!(w <= 200);
        assert!(h <= 200);
    }

    #[test]
    fn test_resize_already_small() {
        let img = make_test_image(10, 10);
        let result = resize_image(img, 200, 200);
        assert_eq!(result.dimensions(), (10, 10));
    }

    // ---- build_key additional tests ----

    #[test]
    fn test_build_key_no_folder_no_conflict() {
        assert_eq!(build_key("", "photo", "jpg", false, false), "photo.jpg");
    }

    #[test]
    fn test_build_key_force_conflict_root() {
        assert_eq!(build_key("", "photo", "jpg", true, true), "photo.jpg");
    }

    #[test]
    fn test_build_key_deeply_nested_folder() {
        assert_eq!(
            build_key("a/b/c/d", "pic", "webp", true, false),
            "a/b/c/d/pic.webp"
        );
    }

    #[test]
    fn test_build_key_conflict_postfix_format() {
        let key = build_key("album", "sun", "png", false, true);
        assert!(key.starts_with("album/sun_"));
        assert!(key.ends_with(".png"));
        let stem = key.strip_prefix("album/sun_").unwrap();
        let stem = stem.strip_suffix(".png").unwrap();
        assert_eq!(stem.len(), 8);
        assert!(stem
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
    }

    #[test]
    fn test_build_key_folder_all_slashes() {
        assert_eq!(build_key("///", "photo", "jpg", true, false), "/photo.jpg");
    }

    #[test]
    fn test_build_key_folder_multiple_trailing_slashes() {
        assert_eq!(
            build_key("photos///", "photo", "jpg", true, false),
            "photos/photo.jpg"
        );
    }

    // ---- config_search_paths tests ----

    #[test]
    fn test_config_search_paths_all_end_with_config_ini() {
        for p in config_search_paths() {
            assert!(
                p.ends_with("config.ini"),
                "path does not end with config.ini: {}",
                p.display()
            );
        }
    }

    #[test]
    fn test_config_search_paths_no_duplicates() {
        let paths = config_search_paths();
        let str_paths: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let mut unique = str_paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(str_paths.len(), unique.len(), "duplicate paths found");
    }

    #[test]
    fn test_config_search_paths_cwd_included() {
        let cwd = std::env::current_dir().unwrap();
        let paths = config_search_paths();
        assert!(
            paths.iter().any(|p| p.parent() == Some(&cwd)),
            "current directory not in search paths"
        );
    }

    #[test]
    fn test_config_search_paths_count_at_least_two() {
        let paths = config_search_paths();
        assert!(
            paths.len() >= 2,
            "expected at least 2 search paths, got {}",
            paths.len()
        );
    }

    // ---- find_config_file additional tests ----

    #[test]
    fn test_find_config_prefers_explicit_over_search() {
        let dir = tempfile::tempdir().unwrap();
        let explicit = dir.path().join("explicit.ini");
        std::fs::write(
            &explicit,
            "[aws]\naccess_key_id=E\nsecret_access_key=S\nbucket=B\n[defaults]\nmax_width=1\nmax_height=2\n",
        )
        .unwrap();
        let found = find_config_file(Some(&explicit)).unwrap();
        assert_eq!(found, explicit);
    }

    #[test]
    fn test_find_config_none_returns_first_search_hit() {
        let found = find_config_file(None);
        if let Ok(path) = found {
            assert!(path.exists());
            assert!(path.to_string_lossy().contains("config.ini"));
        }
    }

    #[test]
    fn test_find_config_none_no_config_found_error() {
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = find_config_file(None);
        std::env::set_current_dir(&orig).unwrap();
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("No config file found"), "unexpected: {}", msg);
        }
    }

    // ---- process_image extension tests ----

    #[test]
    fn test_process_image_bmp_defaults_to_jpeg_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.bmp");
        let img = make_test_image(100, 100);
        img.write_to(
            &mut std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
            image::ImageFormat::Bmp,
        )
        .unwrap();

        let (bytes, ext, w, h) = process_image(&path, 1920, 1080).unwrap();
        assert_eq!(ext, "bmp");
        assert_eq!((w, h), (100, 100));
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }

    // ---- apply_orientation pixel correctness ----

    fn make_pixel_image() -> DynamicImage {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        img.put_pixel(0, 1, image::Rgb([0, 0, 255]));
        img.put_pixel(1, 1, image::Rgb([255, 255, 0]));
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn test_apply_orientation_2_fliph() {
        let img = make_pixel_image();
        let out = apply_orientation(img, 2);
        let raw = out.to_rgb8();
        assert_eq!(raw.get_pixel(0, 0).0, [0, 255, 0]);
        assert_eq!(raw.get_pixel(1, 0).0, [255, 0, 0]);
        assert_eq!(raw.get_pixel(0, 1).0, [255, 255, 0]);
        assert_eq!(raw.get_pixel(1, 1).0, [0, 0, 255]);
    }

    #[test]
    fn test_apply_orientation_3_rotate180() {
        let img = make_pixel_image();
        let out = apply_orientation(img, 3);
        let raw = out.to_rgb8();
        assert_eq!(raw.get_pixel(0, 0).0, [255, 255, 0]);
        assert_eq!(raw.get_pixel(1, 0).0, [0, 0, 255]);
        assert_eq!(raw.get_pixel(0, 1).0, [0, 255, 0]);
        assert_eq!(raw.get_pixel(1, 1).0, [255, 0, 0]);
    }

    #[test]
    fn test_apply_orientation_4_flipv() {
        let img = make_pixel_image();
        let out = apply_orientation(img, 4);
        let raw = out.to_rgb8();
        assert_eq!(raw.get_pixel(0, 0).0, [0, 0, 255]);
        assert_eq!(raw.get_pixel(1, 0).0, [255, 255, 0]);
        assert_eq!(raw.get_pixel(0, 1).0, [255, 0, 0]);
        assert_eq!(raw.get_pixel(1, 1).0, [0, 255, 0]);
    }

    #[test]
    fn test_apply_orientation_invalid_is_noop() {
        let img = make_pixel_image();
        let out = apply_orientation(img, 99);
        let raw = out.to_rgb8();
        assert_eq!(raw.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(raw.get_pixel(1, 1).0, [255, 255, 0]);
    }

    // ---- process_image orientation + resize combined ----

    #[test]
    fn test_process_image_orientation6_with_resize() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tall.jpg");
        write_exif_jpeg(&path, 200, 4000, 6);

        let (bytes, ext, w, h) = process_image(&path, 1000, 1000).unwrap();
        assert_eq!(ext, "jpg");
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.dimensions(), (w, h));
        assert!(w <= 1000 && h <= 1000);
    }

    // ---- load_config missing max_width ----

    #[test]
    fn test_load_config_missing_max_width() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_height = 20\n",
        );
        let result = load_config(f.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("max_width"), "unexpected: {}", msg);
    }

    // ---- low priority tests ----

    #[test]
    fn test_config_search_paths_linux_home_prefix() {
        if cfg!(target_os = "linux") {
            if let Some(home) = dirs::home_dir() {
                let expected = home.join(".config/aws-photo-uploader/config.ini");
                let paths = config_search_paths();
                assert!(
                    paths.contains(&expected),
                    "Linux home config path not found in search paths"
                );
            }
        }
    }

    #[test]
    fn test_build_key_empty_file_stem() {
        let key = build_key("photos", "", "jpg", true, false);
        assert_eq!(key, "photos/.jpg");
    }

    // ---- upload_original / strip_exif config tests ----

    #[test]
    fn test_load_config_upload_original_defaults_to_no() {
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
        let cfg = load_config(f.path()).unwrap();
        assert!(!cfg.upload_original);
    }

    #[test]
    fn test_load_config_upload_original_yes() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n\
             upload_original = yes\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert!(cfg.upload_original);
    }

    #[test]
    fn test_load_config_strip_exif_defaults_to_yes() {
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
        let cfg = load_config(f.path()).unwrap();
        assert!(cfg.strip_exif);
    }

    #[test]
    fn test_load_config_strip_exif_asis() {
        let f = write_config(
            "[aws]\n\
             access_key_id = K\n\
             secret_access_key = S\n\
             bucket = B\n\
             \n\
             [defaults]\n\
             max_width = 10\n\
             max_height = 20\n\
             strip_exif = asis\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert!(!cfg.strip_exif);
    }

    // ---- strip_exif_from_jpeg tests ----

    #[test]
    fn test_strip_exif_from_jpeg_with_exif() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exif.jpg");
        write_exif_jpeg(&path, 100, 100, 1);
        let bytes = std::fs::read(&path).unwrap();
        let stripped = strip_exif_from_jpeg(&bytes);
        assert!(stripped.len() < bytes.len());
        assert_eq!(stripped[0], 0xFF);
        assert_eq!(stripped[1], 0xD8);
        let exif_dir = tempfile::tempdir().unwrap();
        let stripped_path = exif_dir.path().join("stripped.jpg");
        std::fs::write(&stripped_path, &stripped).unwrap();
        let img = image::open(&stripped_path).unwrap();
        assert_eq!(img.dimensions(), (100, 100));
    }

    #[test]
    fn test_strip_exif_from_jpeg_without_exif() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("noexif.jpg");
        let img = make_test_image(50, 50);
        img.write_to(
            &mut std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let stripped = strip_exif_from_jpeg(&bytes);
        assert_eq!(stripped.len(), bytes.len());
    }

    #[test]
    fn test_strip_exif_from_non_jpeg_bytes() {
        let data = b"not a jpeg file";
        let result = strip_exif_from_jpeg(data);
        assert_eq!(result, data);
    }

    // ---- original_bytes tests ----

    #[test]
    fn test_original_bytes_with_strip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        write_exif_jpeg(&path, 100, 100, 1);
        let bytes_before = std::fs::read(&path).unwrap();
        let result = original_bytes(&path, true).unwrap();
        assert!(result.len() < bytes_before.len());
    }

    #[test]
    fn test_original_bytes_without_strip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        write_exif_jpeg(&path, 100, 100, 1);
        let bytes_before = std::fs::read(&path).unwrap();
        let result = original_bytes(&path, false).unwrap();
        assert_eq!(result.len(), bytes_before.len());
    }

    #[test]
    fn test_original_bytes_png_ignored_by_strip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        let img = make_test_image(50, 50);
        img.write_to(
            &mut std::io::BufWriter::new(std::fs::File::create(&path).unwrap()),
            image::ImageFormat::Png,
        )
        .unwrap();
        let bytes_before = std::fs::read(&path).unwrap();
        let result = original_bytes(&path, true).unwrap();
        assert_eq!(result.len(), bytes_before.len());
    }
}
