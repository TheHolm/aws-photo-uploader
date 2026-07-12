# Tests

## load_config

- **test_load_config_valid** — Parses a complete config file with all required fields and verifies each value
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
