# photo-uploader

A Rust command-line tool to upload photos to AWS S3 or S3-compatible storage with automatic resizing/rotation and EXIF stripping.

# THIS IS VIBE CODED GARBAGE, USE ON YOUR OWN RISK

I did glance though the code; it seems to be doing what is expected. But there is no guarantee that it isn't sending your photos to the FBI, too or wipe your entire hard drive. (Not as if that would be a problem, as AWS will do it anyway.)

## How it works

1. Parses CLI args: `photo-uploader <IMAGE> [FOLDER] [-c config.ini] [-f] [--upload-original]`
2. Reads `config.ini` with `[aws]` section (credentials, bucket, region) and `[defaults]` (max_width, max_height, default_folder, upload_original, strip_exif)
3. Reads EXIF orientation from the original image and corrects rotation/flip
4. Loads image, resizes to fit within max dimensions (preserving aspect ratio)
5. Re-encodes image to strip EXIF data (re-encoding discards all metadata)
6. Checks if file exists in S3 via `head_object`; if yes, appends `_xxxxxxxx` random postfix unless -f flag is used
7. Uploads and prints `s3://bucket/key` (or HTML `<img>` tag if `base_url` is set)
8. If `upload_original` is enabled, uploads the original image as `{key}_orig.{ext}` and outputs an additional S3 link or wraps the `<img>` in an `<a>` tag linking to the original

## Usage

```bash
photo-uploader photo.jpg                          # upload to default folder
photo-uploader photo.jpg photos                   # upload to "photos" subfolder
photo-uploader photo.jpg -c /path/to/config.ini   # explicit config path
photo-uploader photo.jpg photos -c config.ini     # explicit config + subfolder
photo-uploader photo.jpg -f                       # force overwrite if exists on destination
photo-uploader photo.jpg --upload-original        # also upload the original alongside resized
```

## config.ini format

```ini
[aws]
access_key_id = YOUR_KEY
secret_access_key = YOUR_SECRET
region = us-east-1
bucket = my-bucket
endpoint_url = https://minio.example.com   # optional, default: AWS S3
storage_class = STANDARD                   # optional, default: STANDARD
base_url = https://cdn.example.com/images  # optional, output HTML instead of S3 path

[defaults]
max_width = 1920
max_height = 1080
default_folder = photos
upload_original = no                       # optional, "yes" to also upload original
strip_exif = yes                           # optional, "asis" to keep original EXIF data
```

The `FOLDER` argument overrides `default_folder` from config. If both are omitted, files are uploaded to the bucket root.

## HTML output with base_url

When `base_url` is set in config, the tool outputs an HTML `<img>` tag instead of the S3 path after upload:

```
<img src="https://cdn.example.com/images/photos/sunset.jpg" alt="sunset" width=1920 height=1080>
```

The `src` URL is constructed as `{base_url}/{folder}/{key}`. This is useful when integrating with static site generators or CMS systems that need direct image references.

## Uploading originals

When `upload_original = yes` is set in config (or `--upload-original` is passed on the CLI), the tool uploads the original image alongside the resized version. The original is named `{resized_key}_orig.{original_extension}`.

- `strip_exif = yes` (default): EXIF data is stripped from the original JPEG (other formats are uploaded as-is)
- `strip_exif = asis`: original is uploaded without any modification

**Without `base_url`**, two S3 paths are printed:
```
s3://my-bucket/photos/sunset.jpg
s3://my-bucket/photos/sunset_orig.jpg
```

**With `base_url`**, the resized image is wrapped in a link to the original:
```html
<a href="https://cdn.example.com/images/photos/sunset_orig.jpg"><img src="https://cdn.example.com/images/photos/sunset.jpg" alt="sunset" width=1920 height=1080></a>
```

## Config file search order

When `-c` is not specified, the application searches for `config.ini` in these locations (first match wins):

**Linux:**
1. `~/.config/aws-photo-uploader/config.ini`
2. `./config.ini` (current directory)
3. `<binary_dir>/config.ini` (next to the executable)

**macOS:**
1. `~/Library/Application Support/aws-photo-uploader/config.ini`
2. `./config.ini`
3. `<binary_dir>/config.ini`

**Windows:**
1. `%APPDATA%/aws-photo-uploader/config.ini`
2. `.\config.ini`
3. `<binary_dir>\config.ini`

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

Binaries are built for Linux, macOS, and Windows via GitHub Actions. Mac and Windows versions are best-effort and not tested.
Download the appropriate binary from the Releases on the [releases page](./releases).

## ToDo

- ~~Add new parameter to config.ini "base_url". If present, program should return HTML image reference instead of S3 path. Format as `<img src="base_url/path_within_bucket" alt="name of the original file without extension" width=xxx height=yyy>`~~ Done.
- ~~Add two new parameters to config.ini and new command line argument to also upload original. First parameter "upload_original": possible values "no" - do not upload (default if omitted), "yes" - upload originals. Second parameter controls stripping EXIF data from original: values "yes" (default if omitted) - strip all EXIF data except orientation, "asis" - upload original as-is. In the bucket originals should be named as "resized_image_name"_orig."extension". If "base_url" not present just return 2 lines with S3 links to resized image and original. If "base_url" present in the config it should return HTML with `<img>` to resized image wrapped to `<a href=>` `</a>` of original.~~ Done.
