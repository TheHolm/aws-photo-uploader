# photo-uploader

A Rust command-line tool to upload photos to AWS S3 with automatic resizing and EXIF stripping.

# THIS IS VIBE CODED GARBAGE, USE ON YOUR OWN RISK

I did glance though the code; it seems to be doing what is expected. But there is no guarantee that it isn't sending your photos to the FBI, too. (Not as if that would be a problem, as AWS will do it anyway.)     
**THIS IS VIBE CODED GARBAGE, USE ON YOUR OWN RISK**

## How it works

1. Parses CLI args: `photo-uploader <IMAGE> [FOLDER] [-c config.ini]`
2. Reads `config.ini` with `[aws]` section (credentials, bucket, region) and `[defaults]` (max_width, max_height, default_folder)
3. Loads image, resizes to fit within max dimensions (preserving aspect ratio)
4. Re-encodes image to strip EXIF data (re-encoding discards all metadata)
5. Checks if file exists in S3 via `head_object`; if yes, appends `_xxxxxxxx` random postfix
6. Uploads and prints `s3://bucket/key`

## Usage

```bash
cargo run -- photo.jpg                         # uses config.ini
cargo run -- photo.jpg photos                   # upload to "photos" subfolder
cargo run -- photo.jpg photos -c my-config.ini  # custom config + subfolder
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
default_folder = photos
```

The `FOLDER` argument overrides `default_folder` from config. If both are omitted, files are uploaded to the bucket root.

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
