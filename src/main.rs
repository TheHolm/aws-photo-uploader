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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.image.exists() {
        bail!("Image file not found: {}", cli.image.display());
    }

    let config_path = find_config_file(cli.config.as_deref())?;
    let config = load_config(&config_path)?;

    let (image_bytes, ext) = process_image(&cli.image, config.max_width, config.max_height)?;

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

    println!("s3://{}/{}", config.bucket, final_key);

    Ok(())
}
