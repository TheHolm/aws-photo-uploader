use image::{DynamicImage, GenericImageView, RgbImage};
use photo_uploader::*;
use std::path::Path;

fn make_test_image(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::new(w, h))
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

fn write_jpeg(path: &Path, w: u32, h: u32) {
    let img = make_test_image(w, h);
    let mut f = std::fs::File::create(path).unwrap();
    img.write_to(
        &mut std::io::BufWriter::new(&mut f),
        image::ImageFormat::Jpeg,
    )
    .unwrap();
}

fn write_png(path: &Path, w: u32, h: u32) {
    let img = make_test_image(w, h);
    let mut f = std::fs::File::create(path).unwrap();
    img.write_to(
        &mut std::io::BufWriter::new(&mut f),
        image::ImageFormat::Png,
    )
    .unwrap();
}

fn write_gif(path: &Path, w: u32, h: u32) {
    let img = make_test_image(w, h);
    let mut f = std::fs::File::create(path).unwrap();
    img.write_to(
        &mut std::io::BufWriter::new(&mut f),
        image::ImageFormat::Gif,
    )
    .unwrap();
}

fn write_webp(path: &Path, w: u32, h: u32) {
    let img = make_test_image(w, h);
    let mut f = std::fs::File::create(path).unwrap();
    img.write_to(
        &mut std::io::BufWriter::new(&mut f),
        image::ImageFormat::WebP,
    )
    .unwrap();
}

#[test]
fn test_pipeline_small_jpeg_no_resize() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.jpg");
    write_jpeg(&path, 100, 80);

    let (bytes, ext, w, h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(ext, "jpg");
    assert!(!bytes.is_empty());
    assert_eq!((w, h), (100, 80));

    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.dimensions(), (100, 80));
}

#[test]
fn test_pipeline_large_jpeg_resized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.jpg");
    write_jpeg(&path, 4000, 3000);

    let (bytes, ext, pw, ph) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(ext, "jpg");
    assert!(!bytes.is_empty());

    let decoded = image::load_from_memory(&bytes).unwrap();
    let (w, h) = decoded.dimensions();
    assert_eq!((pw, ph), (w, h));
    let bound = 1920;
    assert!(
        w <= bound && h <= bound,
        "both {w}x{h} should be <= {bound}"
    );
}

#[test]
fn test_pipeline_exif_orientation_corrected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rotated.jpg");
    write_exif_jpeg(&path, 100, 200, 6);

    let (bytes, _ext, pw, ph) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(
        (pw, ph),
        (200, 100),
        "process_image should return correct dimensions"
    );
    let decoded = image::load_from_memory(&bytes).unwrap();
    let (w, h) = decoded.dimensions();
    assert_eq!((w, h), (200, 100), "orientation 6 should swap dimensions");
}

#[test]
fn test_pipeline_exif_stripped_from_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("with_exif.jpg");
    write_exif_jpeg(&path, 100, 100, 6);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    let reader = std::io::Cursor::new(&bytes);
    let exif_data = exif::Reader::new().read_from_container(&mut std::io::BufReader::new(reader));
    if let Ok(exif) = exif_data {
        assert!(
            exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .is_none(),
            "EXIF orientation should be stripped"
        );
    }
}

#[test]
fn test_pipeline_all_exif_orientations() {
    let dir = tempfile::tempdir().unwrap();
    for orientation in 1..=8 {
        let path = dir.path().join(format!("o{orientation}.jpg"));
        write_exif_jpeg(&path, 100, 200, orientation);

        let (bytes, _ext, pw, ph) = process_image(&path, 1920, 1080).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        let (w, h) = decoded.dimensions();
        assert_eq!((pw, ph), (w, h));

        let (expected_w, expected_h) = match orientation {
            5..=8 => (200, 100),
            _ => (100, 200),
        };
        assert_eq!(
            (w, h),
            (expected_w, expected_h),
            "orientation {orientation} produced {w}x{h}, expected {expected_w}x{expected_h}"
        );
    }
}

#[test]
fn test_pipeline_png_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.png");
    write_png(&path, 100, 100);

    let (bytes, ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(ext, "png");
    assert!(!bytes.is_empty());

    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.dimensions(), (100, 100));
}

#[test]
fn test_pipeline_gif_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("anim.gif");
    write_gif(&path, 50, 50);

    let (bytes, ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(ext, "gif");
    assert!(!bytes.is_empty());

    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.dimensions(), (50, 50));
}

#[test]
fn test_pipeline_exact_dimensions_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exact.jpg");
    write_jpeg(&path, 1920, 1080);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.dimensions(), (1920, 1080));
}

#[test]
fn test_pipeline_nonexistent_file_errors() {
    let result = process_image(Path::new("/nonexistent/photo.jpg"), 1920, 1080);
    assert!(result.is_err());
}

#[test]
fn test_pipeline_asymmetric_max_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.jpg");
    write_jpeg(&path, 3000, 500);

    let (bytes, _ext, _w, _h) = process_image(&path, 800, 200).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    let (w, h) = decoded.dimensions();
    assert!(w <= 800, "width {w} should be <= 800");
    assert!(h <= 200, "height {h} should be <= 200");
}

#[test]
fn test_pipeline_output_is_valid_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.jpg");
    write_jpeg(&path, 100, 100);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(bytes[0], 0xFF);
    assert_eq!(bytes[1], 0xD8);
    assert_eq!(bytes[2], 0xFF);
}

#[test]
fn test_pipeline_output_is_valid_png() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.png");
    write_png(&path, 100, 100);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(bytes[0], 0x89);
    assert_eq!(bytes[1], 0x50);
    assert_eq!(bytes[2], 0x4E);
    assert_eq!(bytes[3], 0x47);
}

#[test]
fn test_content_type_integration() {
    assert_eq!(content_type_for("photo.jpg"), "image/jpeg");
    assert_eq!(content_type_for("photo.jpeg"), "image/jpeg");
    assert_eq!(content_type_for("image.png"), "image/png");
    assert_eq!(content_type_for("anim.gif"), "image/gif");
    assert_eq!(content_type_for("pic.webp"), "image/webp");
    assert_eq!(content_type_for("file.xyz"), "application/octet-stream");
}

#[test]
fn test_build_key_integration() {
    assert_eq!(
        build_key("photos", "sunset", "jpg", false, false),
        "photos/sunset.jpg"
    );
    assert_eq!(
        build_key("photos", "sunset", "jpg", true, true),
        "photos/sunset.jpg"
    );
    let conflict_key = build_key("photos", "sunset", "jpg", false, true);
    assert!(conflict_key.starts_with("photos/sunset_"));
    assert!(conflict_key.ends_with(".jpg"));
    assert_eq!(
        conflict_key.len(),
        "photos/sunset_".len() + 8 + ".jpg".len()
    );
}

#[test]
fn test_pipeline_wide_image_preserves_aspect_ratio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.jpg");
    write_jpeg(&path, 2000, 500);

    let (bytes, _ext, _w, _h) = process_image(&path, 1000, 1000).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    let (w, h) = decoded.dimensions();
    let ratio = w as f64 / h as f64;
    assert!(
        (ratio - 4.0).abs() < 0.1,
        "expected ~4:1 ratio, got {ratio}"
    );
}

#[test]
fn test_pipeline_tall_image_preserves_aspect_ratio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tall.jpg");
    write_jpeg(&path, 500, 2000);

    let (bytes, _ext, _w, _h) = process_image(&path, 1000, 1000).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    let (w, h) = decoded.dimensions();
    let ratio = h as f64 / w as f64;
    assert!(
        (ratio - 4.0).abs() < 0.1,
        "expected ~4:1 ratio, got {ratio}"
    );
}

#[test]
fn test_process_image_returns_correct_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dim.jpg");
    write_jpeg(&path, 640, 480);

    let (_bytes, _ext, w, h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!((w, h), (640, 480));
}

#[test]
fn test_process_image_returns_resized_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.jpg");
    write_jpeg(&path, 3840, 2160);

    let (_bytes, _ext, w, h) = process_image(&path, 1920, 1080).unwrap();
    assert!(w <= 1920 && h <= 1080);
}

#[test]
fn test_process_image_dimensions_match_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("match.jpg");
    write_jpeg(&path, 500, 300);

    let (bytes, _ext, w, h) = process_image(&path, 1920, 1080).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((w, h), decoded.dimensions());
}

#[test]
fn test_pipeline_webp_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.webp");
    write_webp(&path, 100, 80);

    let (bytes, ext, w, h) = process_image(&path, 1920, 1080).unwrap();
    assert_eq!(ext, "webp");
    assert!(!bytes.is_empty());
    assert_eq!((w, h), (100, 80));
}

#[test]
fn test_pipeline_no_extension_defaults_to_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo");
    write_jpeg(&path, 100, 100);

    let result = process_image(&path, 1920, 1080);
    assert!(
        result.is_err(),
        "should fail: image crate cannot detect format without extension"
    );
}

#[test]
fn test_pipeline_non_image_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake.jpg");
    std::fs::write(&path, b"this is not an image").unwrap();

    let result = process_image(&path, 1920, 1080);
    assert!(result.is_err());
}

#[test]
fn test_pipeline_webp_output_magic_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.webp");
    write_webp(&path, 100, 100);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert!(bytes.len() >= 12);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[test]
fn test_pipeline_gif_output_magic_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.gif");
    write_gif(&path, 100, 100);

    let (bytes, _ext, _w, _h) = process_image(&path, 1920, 1080).unwrap();
    assert!(bytes.len() >= 6);
    let header = &bytes[0..6];
    assert!(
        header == b"GIF89a" || header == b"GIF87a",
        "unexpected GIF header: {:?}",
        header
    );
}
