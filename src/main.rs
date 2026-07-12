use anyhow::{bail, Context, Result};
use aws_sdk_s3::Client;
use clap::Parser;
use image::GenericImageView;
use rand::Rng;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "photo-uploader", about = "Upload photos to AWS S3 with resizing")]
struct Cli {
    /// Path to the image file
    image: PathBuf,

    /// Path to config file (default: config.ini)
    #[arg(short, long, default_value = "config.ini")]
    config: PathBuf,
}

pub struct Config {
    access_key_id: String,
    secret_access_key: String,
    region: String,
    bucket: String,
    max_width: u32,
    max_height: u32,
}

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
    })
}

pub fn resize_image(img: image::DynamicImage, max_w: u32, max_h: u32) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_w && h <= max_h {
        return img;
    }
    img.resize(max_w.max(max_h), max_w.max(max_h), image::imageops::FilterType::Lanczos3)
}

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

pub fn content_type_for(path: &str) -> &str {
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.image.exists() {
        bail!("Image file not found: {}", cli.image.display());
    }

    let config = load_config(&cli.config)?;

    let img = image::open(&cli.image)
        .with_context(|| format!("Failed to open image: {}", cli.image.display()))?;

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

    let key_base = format!("{}.{}", file_stem, ext);
    let content_type = content_type_for(&key_base);

    let final_key = {
        let head = client
            .head_object()
            .bucket(&config.bucket)
            .key(&key_base)
            .send()
            .await;
        if head.is_ok() {
            let postfix = random_postfix(8);
            format!("{}_{}.{}", file_stem, postfix, ext)
        } else {
            key_base.clone()
        }
    };

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
             max_height = 600\n",
        );
        let cfg = load_config(f.path()).unwrap();
        assert_eq!(cfg.access_key_id, "AKIA123");
        assert_eq!(cfg.secret_access_key, "secret456");
        assert_eq!(cfg.region, "eu-west-1");
        assert_eq!(cfg.bucket, "my-bucket");
        assert_eq!(cfg.max_width, 800);
        assert_eq!(cfg.max_height, 600);
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
}
