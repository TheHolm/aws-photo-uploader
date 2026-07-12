# photo-uploader

A Rust command-line tool to upload photos to AWS S3 with automatic resizing and EXIF stripping.

## How it works

1. Parses CLI args: `photo-uploader <IMAGE> [-c config.ini]`
2. Reads `config.ini` with `[aws]` section (credentials, bucket, region) and `[defaults]` (max_width, max_height)
3. Loads image, resizes to fit within max dimensions (preserving aspect ratio)
4. Re-encodes image to strip EXIF data (re-encoding discards all metadata)
5. Checks if file exists in S3 via `head_object`; if yes, appends `_xxxxxxxx` random postfix
6. Uploads and prints `s3://bucket/key`

## Usage

```bash
cargo run -- photo.jpg                    # uses config.ini
cargo run -- photo.jpg -c my-config.ini   # custom config path
```

## config.ini format

```ini
[aws]
access_key_id = YOUR_KEY
secret_access_key = YOUR_SECRET
region = us-east-1
bucket = my-bucket

[defaults]
max_width = 1920
max_height = 1080
```

## Minimal IAM policy

The following policy grants only the permissions required by the application:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject"
      ],
      "Resource": "arn:aws:s3:::your-bucket-name/*"
    }
  ]
}
```

- `s3:GetObject` — needed for `HeadObject` to check if a file already exists
- `s3:PutObject` — needed to upload the image

## Build

```bash
cargo build --release
```

Binaries are built for Linux, macOS, and Windows via GitHub Actions.
Download the appropriate binary from the Actions artifacts on the [releases page](../../actions).
