# Tests

## load_config

- **test_load_config_valid** — Parses a complete config file with all required fields and verifies each value
- **test_load_config_with_endpoint_and_storage_class** — Parses optional `endpoint_url` and `storage_class` fields
- **test_load_config_missing_region_defaults** — Omits `region` from config and verifies it defaults to `us-east-1`
- **test_load_config_missing_required_field** — Omits `access_key_id` and verifies an error is returned
- **test_load_config_comments_and_blank_lines** — Config with `;` and `#` comments and blank lines parses correctly
- **test_load_config_case_insensitive** — Section names (`[AWS]`) and keys (`Access_Key_Id`) are matched case-insensitively
- **test_load_config_invalid_number** — `max_width = not_a_number` returns an error
- **test_load_config_file_not_found** — Non-existent path returns an error

## resize_image

- **test_resize_within_bounds** — Image smaller than max dimensions is returned unchanged
- **test_resize_exact_bounds** — Image exactly at max dimensions is returned unchanged
- **test_resize_exceeds_width** — Image wider than max width is resized to fit
- **test_resize_exceeds_height** — Image taller than max height is resized to fit
- **test_resize_exceeds_both** — Image exceeding both dimensions is resized to fit within bounds
- **test_resize_aspect_ratio_preserved** — 1000x500 image resized to 200x200 max preserves ~2:1 aspect ratio
- **test_resize_asymmetric_bounds** — 1000x500 image with max_width=200, max_height=100 stays within both limits
- **test_resize_small_wide_image** — 400x50 image resized with 200x200 bounds fits within limits
- **test_resize_small_tall_image** — 50x400 image resized with 200x200 bounds fits within limits
- **test_resize_already_small** — 10x10 image within 200x200 bounds is returned unchanged

## random_postfix

- **test_random_postfix_length** — Returns a string of the requested length (8)
- **test_random_postfix_length_zero** — Requesting length 0 returns an empty string
- **test_random_postfix_valid_chars** — All characters are ASCII digits or lowercase letters (`[0-9a-z]`)
- **test_random_postfix_uniqueness** — Two consecutive calls with length 16 produce different strings

## content_type_for

- **test_content_type_jpg** — `photo.jpg` returns `image/jpeg`
- **test_content_type_jpeg** — `photo.jpeg` returns `image/jpeg`
- **test_content_type_png** — `image.png` returns `image/png`
- **test_content_type_webp** — `pic.webp` returns `image/webp`
- **test_content_type_gif** — `anim.gif` returns `image/gif`
- **test_content_type_unknown** — `file.xyz` returns `application/octet-stream`
- **test_content_type_no_extension** — `noext` returns `application/octet-stream`
- **test_content_type_case_insensitive** — `photo.JPG`, `photo.Png`, `photo.GiF` match correctly
- **test_content_type_path_with_dirs** — `/some/path/photo.jpg` extracts extension from filename

## build_key

- **test_build_key_no_folder** — Force mode with empty folder returns `photo.jpg`
- **test_build_key_with_folder** — Force mode with folder returns `photos/photo.jpg`
- **test_build_key_nested_folder** — Nested folder `2024/january` produces correct key
- **test_build_key_folder_trailing_slash** — Trailing slash in folder is stripped
- **test_build_key_force_overwrites** — Force=true, object_exists=true returns base key (overwrite)
- **test_build_key_force_no_conflict** — Force=true, object_exists=false returns base key
- **test_build_key_no_force_no_conflict** — Force=false, object_exists=false returns base key
- **test_build_key_conflict_appends_postfix** — Force=false, object_exists=true appends `_xxxxxxxx` postfix
- **test_build_key_conflict_no_folder** — Conflict in root (no folder) appends postfix correctly
- **test_build_key_no_folder_no_conflict** — Root-level, no force, no conflict returns plain filename
- **test_build_key_force_conflict_root** — Force=true, conflict at root returns base key
- **test_build_key_deeply_nested_folder** — 4-level nested folder produces correct key
- **test_build_key_conflict_postfix_format** — Postfix is exactly 8 chars of `[0-9a-z]`

## find_config_file

- **test_find_config_explicit_path** — Explicit path is returned when file exists
- **test_find_config_explicit_not_found** — Explicit non-existent path returns error
- **test_find_config_returns_search_paths** — `config_search_paths()` returns non-empty list with `config.ini` paths
- **test_find_config_prefers_explicit_over_search** — Explicit path is preferred over search paths
- **test_find_config_none_returns_first_search_hit** — With None, returns first existing config from search paths

## config_search_paths

- **test_config_search_paths_all_end_with_config_ini** — All returned paths end with `config.ini`
- **test_config_search_paths_no_duplicates** — No duplicate paths in search results
- **test_config_search_paths_cwd_included** — Current working directory is included in search paths

## apply_orientation

- **test_apply_orientation_normal** — Orientation 1 (normal) returns image unchanged
- **test_apply_orientation_flip_h** — Orientation 2 (flip horizontal) returns same dimensions
- **test_apply_orientation_rotate_180** — Orientation 3 (rotate 180) returns same dimensions
- **test_apply_orientation_flip_v** — Orientation 4 (flip vertical) returns same dimensions
- **test_apply_orientation_transpose** — Orientation 5 (transpose) swaps dimensions
- **test_apply_orientation_rotate_90** — Orientation 6 (rotate 90) swaps dimensions
- **test_apply_orientation_transverse** — Orientation 7 (transverse) swaps dimensions
- **test_apply_orientation_rotate_270** — Orientation 8 (rotate 270) swaps dimensions
- **test_apply_orientation_invalid_defaults_to_noop** — Invalid orientation value returns image unchanged

## read_exif_orientation

- **test_read_exif_nonexistent_file** — Non-existent file returns an error
- **test_read_exif_no_exif_returns_one** — JPEG with no EXIF data returns orientation 1
- **test_read_exif_orientation_1** — Reads back orientation 1 from JPEG with embedded EXIF
- **test_read_exif_orientation_2** — Reads back orientation 2 from JPEG with embedded EXIF
- **test_read_exif_orientation_3** — Reads back orientation 3 from JPEG with embedded EXIF
- **test_read_exif_orientation_4** — Reads back orientation 4 from JPEG with embedded EXIF
- **test_read_exif_orientation_5** — Reads back orientation 5 from JPEG with embedded EXIF
- **test_read_exif_orientation_6** — Reads back orientation 6 from JPEG with embedded EXIF
- **test_read_exif_orientation_7** — Reads back orientation 7 from JPEG with embedded EXIF
- **test_read_exif_orientation_8** — Reads back orientation 8 from JPEG with embedded EXIF
- **test_read_exif_orientation_roundtrip** — Write orientation 6, read back and verify
