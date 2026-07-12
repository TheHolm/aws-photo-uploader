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

struct Config {
    access_key_id: String,
    secret_access_key: String,
    region: String,
    bucket: String,
    max_width: u32,
    max_height: u32,
}

fn load_config(path: &Path) -> Result<Config> {
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

fn resize_image(img: image::DynamicImage, max_w: u32, max_h: u32) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_w && h <= max_h {
        return img;
    }
    img.resize(max_w.max(max_h), max_w.max(max_h), image::imageops::FilterType::Lanczos3)
}

fn random_postfix(len: usize) -> String {
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

fn content_type_for(path: &str) -> &str {
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
