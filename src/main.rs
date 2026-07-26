use anyhow::{bail, Context, Result};
use aws_sdk_s3::Client;
use clap::Parser;
use std::path::PathBuf;

use photo_uploader::*;

#[derive(Parser)]
#[command(
    name = "photo-uploader",
    about = "Upload photos to AWS S3 with resizing"
)]
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

    /// Also upload the original image alongside the resized version
    #[arg(long)]
    upload_original: bool,

    /// Preserve EXIF data in the original upload (overrides strip_exif in config)
    #[arg(long)]
    keep_exif: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.image.exists() {
        bail!("Image file not found: {}", cli.image.display());
    }

    let config_path = find_config_file(cli.config.as_deref())?;
    let config = load_config(&config_path)?;

    let (image_bytes, ext, width, height) =
        process_image(&cli.image, config.max_width, config.max_height)?;

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

    let mut s3_config = aws_sdk_s3::config::Builder::from(&sdk_config);
    if let Some(ref endpoint) = config.endpoint_url {
        s3_config = s3_config.endpoint_url(endpoint);
    }
    let client = Client::from_conf(s3_config.build());

    let file_stem = cli
        .image
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("photo");

    let folder = cli.folder.as_deref().unwrap_or(&config.default_folder);

    let do_upload_original = cli.upload_original || config.upload_original;
    let do_strip_exif = config.strip_exif && !cli.keep_exif;

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

    let mut put = client
        .put_object()
        .bucket(&config.bucket)
        .key(&final_key)
        .body(image_bytes.into())
        .content_type(content_type);
    if let Some(ref class) = config.storage_class {
        put = put.storage_class(aws_sdk_s3::types::StorageClass::from(class.as_str()));
    }
    put.send().await.context("Failed to upload to S3")?;

    let original_key = if do_upload_original {
        let orig_bytes = original_bytes(&cli.image, do_strip_exif)?;
        let orig_ext = cli
            .image
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg")
            .to_lowercase();
        let orig_key = format!(
            "{}_orig.{}",
            final_key
                .strip_suffix(&format!(".{}", ext))
                .unwrap_or(&final_key),
            orig_ext
        );
        let orig_ct = content_type_for(&orig_key);
        let mut orig_put = client
            .put_object()
            .bucket(&config.bucket)
            .key(&orig_key)
            .body(orig_bytes.into())
            .content_type(orig_ct);
        if let Some(ref class) = config.storage_class {
            orig_put =
                orig_put.storage_class(aws_sdk_s3::types::StorageClass::from(class.as_str()));
        }
        orig_put
            .send()
            .await
            .context("Failed to upload original to S3")?;
        Some(orig_key)
    } else {
        None
    };

    if let Some(ref base_url) = config.base_url {
        let alt = file_stem;
        let url = format!(
            "{}/{}/{}",
            base_url.trim_end_matches('/'),
            folder,
            final_key
        );
        if let Some(ref orig_key) = original_key {
            let orig_url = format!("{}/{}/{}", base_url.trim_end_matches('/'), folder, orig_key);
            println!(
                r#"<a href="{}"><img src="{}" alt="{}" width={} height={}></a>"#,
                orig_url, url, alt, width, height
            );
        } else {
            println!(
                r#"<img src="{}" alt="{}" width={} height={}>"#,
                url, alt, width, height
            );
        }
    } else if let Some(ref orig_key) = original_key {
        println!("s3://{}/{}", config.bucket, final_key);
        println!("s3://{}/{}", config.bucket, orig_key);
    } else {
        println!("s3://{}/{}", config.bucket, final_key);
    }

    Ok(())
}
