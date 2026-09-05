use super::keys::{cache_key, display_key};
use super::raster::{decoded_len, raster_display};
use super::source::{
    decode_data_payload, detect_format, is_image_path, markdown_dest, resolve,
    validate_image_dimensions, validate_svg_dimensions,
};
use super::{DataEncoding, ImageCache, Resolved, WARM_DISPLAY_EXTRA};
use crate::images::fit::{contain_fit, width_fit};
use gpui::{ImageFormat, RenderImage};
use std::path::PathBuf;
use std::sync::Arc;

use image::RgbImage;
use smallvec::smallvec;
use std::io::Cursor;

fn tiny_png() -> Vec<u8> {
    png_with_size(4, 3)
}

fn png_with_size(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_pixel(width, height, image::Rgb([255, 0, 0]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("png");
    buf
}

fn native_from_png(png: &[u8]) -> (Arc<RenderImage>, u32, u32) {
    let ready = crate::pixels::decode_png(png, 1.0).expect("png");
    (ready.image, ready.px_w, ready.px_h)
}

#[test]
fn decoded_len_counts_every_animation_frame() {
    let frames = smallvec![
        image::Frame::new(image::RgbaImage::new(2, 3)),
        image::Frame::new(image::RgbaImage::new(2, 3)),
    ];
    let image = RenderImage::new(frames);
    assert_eq!(image.frame_count(), 2);
    assert_eq!(decoded_len(&image), 2 * 2 * 3 * 4);
}

#[test]
fn relative_joins_markdown_parent() {
    let src = PathBuf::from("L:").join("md").join("doc.md");
    let got = resolve("./a.png", Some(&src)).expect("resolve");
    match got {
        Resolved::Local(p) => {
            let expected = std::path::absolute(src.parent().unwrap().join("a.png")).unwrap();
            assert_eq!(p, expected);
        }
        _ => panic!("expected local"),
    }
}

#[test]
fn https_stays_remote_key() {
    let url = "https://example.com/x.png";
    match resolve(url, None).expect("resolve") {
        Resolved::Remote(u) => assert_eq!(u, url),
        _ => panic!("expected remote"),
    }
    assert_eq!(cache_key(url, None), url);
}

#[test]
fn relative_without_source_fails() {
    assert!(resolve("./a.png", None).is_none());
}

#[test]
fn non_ascii_destinations_are_safe_local_paths() {
    let src = PathBuf::from("L:").join("md").join("doc.md");
    for dest in ["café.png", "naïve.png", "schéma/a.png", "résumé.md"] {
        assert!(
            matches!(resolve(dest, Some(&src)), Some(Resolved::Local(_))),
            "{dest:?}"
        );
        assert!(!cache_key(dest, Some(&src)).is_empty(), "{dest:?}");
    }
}

#[test]
fn remote_scheme_matching_stays_ascii_case_insensitive() {
    for url in ["HTTP://example.com/x.png", "HTTPS://example.com/x.png"] {
        assert!(matches!(resolve(url, None), Some(Resolved::Remote(u)) if u == url));
    }
}

#[test]
fn windows_drive_path_is_not_a_uri_scheme() {
    match resolve(r"C:\images\a.png", None) {
        Some(Resolved::Local(p)) => {
            assert!(p.to_string_lossy().contains("a.png"), "{p:?}");
        }
        other => panic!("expected local, got {other:?}"),
    }
    match resolve("C:/images/a.png", None) {
        Some(Resolved::Local(p)) => {
            assert!(p.to_string_lossy().contains("a.png"), "{p:?}");
        }
        other => panic!("expected local, got {other:?}"),
    }
    assert!(resolve("javascript:alert(1)", None).is_none());
}

#[test]
fn relative_file_next_to_markdown_resolves_and_roundtrips() {
    let dir = std::env::temp_dir().join(format!(
        "md-test-img-rel-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("images")).expect("dir");
    let png = dir.join("images").join("a.png");
    std::fs::write(&png, tiny_png()).expect("png");
    let md = dir.join("doc.md");
    std::fs::write(&md, "![x](./images/a.png)\n").expect("md");

    let want = png.canonicalize().expect("png exists");
    for dest in ["./images/a.png", "images/a.png"] {
        let Resolved::Local(p) = resolve(dest, Some(&md)).expect(dest) else {
            panic!("{dest} should be local");
        };
        assert_eq!(p.canonicalize().unwrap_or(p.clone()), want, "{dest}");
        let key = cache_key(dest, Some(&md));
        let Resolved::Local(again) = resolve(&key, Some(&md))
            .unwrap_or_else(|| panic!("cache key {key:?} from {dest} should still resolve"))
        else {
            panic!("cache key {key:?} should be local");
        };
        assert_eq!(
            again.canonicalize().unwrap_or(again),
            want,
            "{dest} key={key}"
        );
    }

    let nested = dir.join("sub");
    std::fs::create_dir_all(&nested).expect("sub");
    let nested_md = nested.join("doc.md");
    std::fs::write(&nested_md, "![x](../images/a.png)\n").expect("nested md");
    let Resolved::Local(p) = resolve("../images/a.png", Some(&nested_md)).expect("../") else {
        panic!("../ should be local");
    };
    assert_eq!(p.canonicalize().unwrap_or(p), want);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn is_image_path_reads_the_extension() {
    assert!(is_image_path(std::path::Path::new("a.PNG")));
    assert!(is_image_path(std::path::Path::new("x.webp")));
    assert!(!is_image_path(std::path::Path::new("notes.md")));
    assert!(!is_image_path(std::path::Path::new("a")));
}

#[test]
fn markdown_dest_is_relative_to_the_markdown_parent() {
    let dir = std::env::temp_dir().join(format!(
        "md-test-img-dest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("images")).expect("dir");
    let png = dir.join("images").join("a.png");
    std::fs::write(&png, tiny_png()).expect("png");
    let md = dir.join("doc.md");
    std::fs::write(&md, "").expect("md");
    let dest = markdown_dest(&png, Some(&md));
    assert_eq!(dest, "images/a.png", "{dest}");
    let sibling = dir.join("b.png");
    std::fs::write(&sibling, tiny_png()).expect("sibling");
    assert_eq!(markdown_dest(&sibling, Some(&md)), "b.png");
    let nested = dir.join("sub");
    std::fs::create_dir_all(&nested).expect("sub");
    let nested_md = nested.join("doc.md");
    std::fs::write(&nested_md, "").expect("nested md");
    assert_eq!(markdown_dest(&png, Some(&nested_md)), "../images/a.png");
    let abs = markdown_dest(&png, None);
    assert!(abs.contains("a.png"), "{abs}");
    assert!(abs.contains('/'), "{abs}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn source_begin_twice_does_not_start_two_fetches() {
    let mut cache = ImageCache::new();
    assert!(cache.begin_source("k".into()));
    assert!(!cache.begin_source("k".into()));
    assert!(cache.source_contains("k"));
}

#[test]
fn display_drop_keeps_source_ready() {
    let png = tiny_png();
    let native = native_from_png(&png);
    let mut cache = ImageCache::new();
    assert!(cache.begin_source("k".into()));
    assert!(cache.finish_source_inner("k".into(), Ok(native.clone()), None, usize::MAX));
    assert_eq!(cache.intrinsic("k"), Some((4, 3)));
    let key = display_key("k", 4.0, 3.0, 1.0);
    assert!(cache.begin_display(key.clone()));
    let ready = raster_display(&native.0, &key).expect("raster");
    assert!(cache.finish_display_inner(key.clone(), Ok(ready), None));
    assert!(cache.display_ready(&key).is_some());
    cache.drop_display_keep_source(&key);
    assert!(cache.display_ready(&key).is_none());
    assert_eq!(cache.intrinsic("k"), Some((4, 3)));
    assert!(cache.source_image("k").is_some());
}

#[test]
fn source_lru_evicts_derived_displays_and_retries_budget_pressure() {
    let native = native_from_png(&tiny_png());
    let source_bytes = native.0.as_bytes(0).expect("pixels").len();
    let mut cache = ImageCache::new();

    let a = display_key("a", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        source_bytes,
    );
    assert!(cache.begin_source("a".into()));
    assert!(cache.finish_source_inner("a".into(), Ok(native.clone()), None, source_bytes));
    assert!(cache.begin_display(a.clone()));
    let display = raster_display(&native.0, &a).expect("display");
    assert!(cache.finish_display_inner(a.clone(), Ok(display), None));

    let b = display_key("b", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        source_bytes,
    );
    assert!(cache.begin_source("b".into()));
    assert!(cache.finish_source_inner("b".into(), Ok(native.clone()), None, source_bytes));
    assert!(!cache.source_contains("a"));
    assert!(cache.source_contains("b"));
    assert!(cache.display_ready(&a).is_none());
    assert_eq!(cache.intrinsic("a"), None);
    assert!(cache.source_bytes() <= source_bytes);

    let c = display_key("c", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [b, c],
        std::iter::empty(),
        std::iter::empty(),
        None,
        source_bytes,
    );
    assert!(cache.begin_source("c".into()));
    assert!(cache.finish_source_inner("c".into(), Ok(native), None, source_bytes));
    assert!(cache.source_contains("c"));
    assert!(cache.source_error("c").is_some());
    cache.set_working_set_inner(
        std::iter::empty(),
        ["b".to_string()],
        std::iter::empty(),
        None,
        source_bytes,
    );
    assert!(!cache.source_contains("c"));
}

#[test]
fn visible_over_budget_retries_once_offscreen_space_covers_it() {
    let native = native_from_png(&tiny_png());
    let n = native.0.as_bytes(0).expect("pixels").len();

    let budget = n + n / 2;
    let mut cache = ImageCache::new();

    let a = display_key("a", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    assert!(cache.begin_source("a".into()));
    assert!(cache.finish_source_inner("a".into(), Ok(native.clone()), None, budget));

    let b = display_key("b", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone(), b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    assert!(cache.begin_source("b".into()));
    assert!(cache.finish_source_inner("b".into(), Ok(native.clone()), None, budget));
    assert!(cache.source_error("b").is_some());

    cache.set_working_set_inner(
        [a, b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    assert!(
        cache.source_contains("b"),
        "a visible OverBudget must not keep getting cleared while it still cannot fit"
    );

    cache.set_working_set_inner([b], std::iter::empty(), std::iter::empty(), None, budget);
    assert!(
        !cache.source_contains("b"),
        "once space is freed, the OverBudget slot must be cleared"
    );
    assert!(cache.begin_source("b".into()));
    assert!(cache.finish_source_inner("b".into(), Ok(native), None, budget));
    assert_eq!(cache.intrinsic("b"), Some((4, 3)));
    assert!(
        !cache.source_contains("a"),
        "booking the retry must evict the offscreen a"
    );
}

#[test]
fn image_larger_than_the_whole_budget_never_retries() {
    let native = native_from_png(&png_with_size(6, 5));
    let mut cache = ImageCache::new();
    let k = display_key("k", 6.0, 5.0, 1.0);
    cache.set_working_set_inner(
        [k.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        48,
    );
    assert!(cache.begin_source("k".into()));
    assert!(cache.finish_source_inner("k".into(), Ok(native), None, 48));
    assert!(cache.source_error("k").is_some());

    cache.set_working_set_inner([k], std::iter::empty(), std::iter::empty(), None, 48);
    assert!(cache.source_contains("k"));
}

#[test]
fn raster_downscales_from_source_pixels() {
    let png = png_with_size(400, 300);
    let native = native_from_png(&png);
    let key = display_key("k", 256.0, 192.0, 1.0);
    let ready = raster_display(&native.0, &key).expect("raster");
    assert_eq!((ready.px_w, ready.px_h), (256, 192));
    assert!((ready.px_w as f32 / ready.px_h as f32 - 4.0 / 3.0).abs() < 1e-5);
    assert!(ready.bytes > 0);
}

#[test]
fn display_keys_share_a_bucket_for_nearby_slots() {
    let first = display_key("k", 256.0, 192.0, 1.0);
    let nearby = display_key("k", 287.9, 223.9, 1.0);
    assert_eq!(first, nearby);
    assert_eq!(first.raster_slot(), (256.0, 192.0));
}

#[test]
fn display_key_floors_slots_to_a_stable_raster_target() {
    let exact = display_key("k", 800.0, 640.0, 1.5);
    let same_bucket = display_key("k", 815.9, 655.9, 1.5);
    assert_eq!(exact, same_bucket);
    assert_eq!(exact.raster_slot(), (800.0, 640.0));
    assert_eq!(exact.raster_dpr(), 1.5);

    let narrower = display_key("k", 799.9, 639.9, 1.5);
    assert_eq!(narrower.raster_slot(), (768.0, 608.0));
    assert!(narrower.raster_slot().0 <= 799.9);
    assert!(narrower.raster_slot().1 <= 639.9);
}

#[test]
fn display_key_keeps_small_slots_in_the_first_raster_bucket() {
    let key = display_key("icon", 16.0, 12.0, 1.0);
    assert_eq!((key.width_q, key.height_q), (1, 1));
    assert_eq!(key.raster_slot(), (32.0, 32.0));
}

#[test]
fn width_fit_keeps_aspect_and_does_not_upsize() {
    assert_eq!(width_fit(1600.0, 900.0, 800.0), (800.0, 450.0));
    assert_eq!(width_fit(100.0, 200.0, 800.0), (100.0, 200.0));
}

#[test]
fn contain_fit_still_shrinks_to_both_axes() {
    let (w, h) = contain_fit(1600.0, 900.0, 800.0, 200.0);
    assert!((h - 200.0).abs() < 1e-4);
    assert!((w - 200.0 * 1600.0 / 900.0).abs() < 1e-3);
}

#[test]
fn data_url_resolves() {
    let png = tiny_png();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    let dest = format!("data:image/png;base64,{b64}");
    match resolve(&dest, None).expect("data") {
        Resolved::Data {
            payload,
            format,
            encoding,
        } => {
            assert_eq!(payload, b64);
            assert_eq!(encoding, DataEncoding::Base64);
            assert_eq!(
                decode_data_payload(&payload, encoding).expect("base64"),
                png
            );
            assert_eq!(format, ImageFormat::Png);
        }
        _ => panic!("expected data"),
    }
}

#[test]
fn data_url_cache_key_is_compact_and_payload_sensitive() {
    let first = "data:image/png;base64,AAAA";
    let second = "data:image/png;base64,AAAB";
    let key = cache_key(first, None);
    assert!(key.starts_with("data:"), "{key}");
    assert_eq!(key.len(), 21);
    assert_eq!(key, cache_key(first, None));
    assert_ne!(key, cache_key(second, None));
    assert_ne!(key, first);
}

#[test]
fn resolving_a_data_url_defers_base64_validation() {
    match resolve("data:image/png;base64,not-base64!", None).expect("data header") {
        Resolved::Data {
            payload,
            format,
            encoding,
        } => {
            assert_eq!(payload, "not-base64!");
            assert_eq!(format, ImageFormat::Png);
            assert_eq!(encoding, DataEncoding::Base64);
            assert!(decode_data_payload(&payload, encoding).is_none());
        }
        _ => panic!("expected data"),
    }
}

#[test]
fn percent_encoded_data_urls_decode_without_base64() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="3"/>"#;
    let encoded =
        percent_encoding::utf8_percent_encode(svg, percent_encoding::NON_ALPHANUMERIC).to_string();
    let dest = format!("data:image/svg+xml,{encoded}");
    match resolve(&dest, None).expect("percent data") {
        Resolved::Data {
            payload,
            format,
            encoding,
        } => {
            assert_eq!(format, ImageFormat::Svg);
            assert_eq!(encoding, DataEncoding::Percent);
            assert_eq!(
                decode_data_payload(&payload, encoding).unwrap(),
                svg.as_bytes()
            );
        }
        _ => panic!("expected data"),
    }
}

#[test]
fn oversized_gif_is_rejected_before_gpui_decode() {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&u16::MAX.to_le_bytes());
    gif.extend_from_slice(&u16::MAX.to_le_bytes());
    gif.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
    let err = validate_image_dimensions(ImageFormat::Gif, &gif)
        .expect_err("oversized GIF must be rejected from its logical screen header");
    assert!(matches!(err, crate::Error::Image(_)));
}

#[test]
fn svg_dimensions_are_checked_before_rasterization() {
    let small = br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"/>"#;
    assert!(validate_svg_dimensions(small).is_ok());

    let bomb = br#"<svg xmlns="http://www.w3.org/2000/svg" width="40000" height="40000"/>"#;
    let err = validate_svg_dimensions(bomb).expect_err("pixel bomb must be rejected");
    assert!(matches!(err, crate::Error::Image(_)));
}

fn png_with_header_only(width: u32, height: u32) -> Vec<u8> {
    let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    png.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52]);
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[0x08, 0x02, 0x00, 0x00, 0x00]);

    let crc = crc32(&png[12..]);
    png.extend_from_slice(&crc.to_be_bytes());

    png.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x49, 0x44, 0x41, 0x54]);
    png.extend_from_slice(&crc32(b"IDAT").to_be_bytes());
    png.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]);
    png
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn real_jpeg_with_declared(width: u32, height: u32, declare: (u32, u32)) -> Vec<u8> {
    let img = image::GrayImage::from_pixel(width, height, image::Luma([128]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageLuma8(img)
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
        .expect("jpeg");
    let sof = buf
        .windows(2)
        .position(|w| w == [0xFF, 0xC0])
        .expect("baseline SOF0");

    buf[sof + 5..sof + 7].copy_from_slice(&(declare.1 as u16).to_be_bytes());
    buf[sof + 7..sof + 9].copy_from_slice(&(declare.0 as u16).to_be_bytes());
    buf
}

#[test]
fn malformed_raster_headers_are_rejected_without_panicking() {
    let full = png_with_size(4, 3);
    for n in [0usize, 4, 8, 16, 24, 32] {
        assert!(
            validate_image_dimensions(ImageFormat::Png, &full[..n]).is_err(),
            "png truncated to {n} bytes must be rejected"
        );
    }
    assert!(validate_image_dimensions(ImageFormat::Jpeg, &[0xFF, 0xD8]).is_err());
    assert!(validate_image_dimensions(ImageFormat::Png, &[]).is_err());
    assert!(validate_image_dimensions(ImageFormat::Png, b"not a png at all").is_err());
}

#[test]
fn oversized_png_pixel_declarations_are_rejected_before_decode() {
    let bomb = png_with_header_only(65535, 65535);
    let err = validate_image_dimensions(ImageFormat::Png, &bomb)
        .expect_err("a 65535^2 pixel declaration must be rejected for its size");
    assert!(matches!(err, crate::Error::Image(_)));

    let zero = png_with_header_only(0, 3);
    let err = validate_image_dimensions(ImageFormat::Png, &zero)
        .expect_err("a 0-width declaration must be rejected");
    assert!(matches!(err, crate::Error::Image(_)));
}

#[test]
fn jpeg_pixel_declarations_above_the_cap_are_rejected() {
    let bomb = real_jpeg_with_declared(9, 7, (40000, 40000));
    assert!(
        validate_image_dimensions(ImageFormat::Jpeg, &bomb).is_err(),
        "a SOF declaration of 40000^2 pixels must be rejected"
    );
    let honest = real_jpeg_with_declared(9, 7, (9, 7));
    assert!(
        validate_image_dimensions(ImageFormat::Jpeg, &honest).is_ok(),
        "the untampered baseline JPEG must still pass"
    );
}

#[test]
fn deeply_nested_svg_groups_are_rejected_before_parsing() {
    let deep = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"3\">{}{}</svg>",
        "<g>".repeat(100_000),
        "</g>".repeat(100_000)
    );
    assert!(
        validate_svg_dimensions(deep.as_bytes()).is_err(),
        "100,000 nested <g> layers must be rejected by the depth prescan before parsing"
    );
    assert!(
        detect_format(deep.as_bytes()).is_none(),
        "the format probe must not mistake it for SVG (same usvg call)"
    );
}

#[test]
fn svg_depth_scanner_skips_comments_cdata_and_quoted_attributes() {
    let noisy = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"3\" data-q=\"1 > 0\">\
         <!-- {} -->\
         <![CDATA[<a><b><c><d><e>]]>\
         </svg>",
        "<g>".repeat(100_000)
    );
    assert!(
        validate_svg_dimensions(noisy.as_bytes()).is_ok(),
        "tag text in comments/CDATA/attribute values must not count toward depth"
    );
}

#[test]
fn truncated_png_bodies_pass_the_guard_and_fail_in_the_decoder() {
    let full = png_with_size(4, 3);
    let truncated = &full[..full.len() - 10];
    assert!(validate_image_dimensions(ImageFormat::Png, truncated).is_ok());

    assert!(
        image::load_from_memory_with_format(truncated, image::ImageFormat::Png).is_err(),
        "a truncated body must fail in the decoder"
    );
}

#[test]
fn format_labels_do_not_override_the_bytes() {
    let jpeg = real_jpeg_with_declared(9, 7, (9, 7));
    assert!(
        validate_image_dimensions(ImageFormat::Png, &jpeg).is_ok(),
        "the guard reads the real header in the bytes (JPEG), not the Png label"
    );
    assert!(
        image::load_from_memory_with_format(&jpeg, image::ImageFormat::Png).is_err(),
        "JPEG bytes labeled PNG must be rejected when forced through the PNG decoder"
    );
}

fn fill_display_ready(cache: &mut ImageCache, n: usize) {
    let native = native_from_png(&tiny_png());
    for i in 0..n {
        let dest = format!("d{i}");
        let key = display_key(&dest, 4.0, 3.0, 1.0);
        assert!(cache.begin_source(dest.clone()));
        assert!(cache.finish_source_inner(dest, Ok(native.clone()), None, usize::MAX));
        assert!(cache.begin_display(key.clone()));
        let display = raster_display(&native.0, &key).expect("display");
        assert!(cache.finish_display_inner(key, Ok(display), None));
    }
}

#[test]
fn set_working_set_trims_offscreen_display_to_the_warm_window() {
    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 24);
    assert_eq!(cache.display_entry_count(), 24);
    let keep = display_key("d23", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [keep.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        usize::MAX,
    );
    assert!(cache.display_ready(&keep).is_some());
    assert_eq!(cache.display_entry_count(), 1 + WARM_DISPLAY_EXTRA);
    assert!(
        cache
            .display_ready(&display_key("d0", 4.0, 3.0, 1.0))
            .is_none()
    );
}

#[test]
fn warm_sources_outlive_cold_ones() {
    let each = native_from_png(&tiny_png())
        .0
        .as_bytes(0)
        .expect("pixels")
        .len();
    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 24);

    let warm = ["d0".to_string(), "d1".to_string(), "d2".to_string()];

    cache.set_working_set_inner(
        [display_key("d23", 4.0, 3.0, 1.0)],
        std::iter::empty(),
        warm.clone(),
        None,
        each * 20,
    );
    assert!(
        cache.source_bytes() < each * 24,
        "the premise of this test is that eviction actually occurred"
    );
    for dest in &warm {
        assert!(
            cache.source_contains(dest),
            "warm sources must not be dropped while cold entries remain"
        );
    }
    assert!(cache.source_contains("d23"));

    assert!(!cache.source_contains("d3"));
}

#[test]
fn warm_displays_ride_along_with_their_sources() {
    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 24);
    let hot = display_key("d23", 4.0, 3.0, 1.0);
    let warm = ["d0".to_string(), "d1".to_string(), "d2".to_string()];
    cache.set_working_set_inner(
        [hot.clone()],
        std::iter::empty(),
        warm.clone(),
        None,
        usize::MAX,
    );

    assert_eq!(
        cache.display_entry_count(),
        1 + warm.len() + WARM_DISPLAY_EXTRA
    );
    for dest in &warm {
        assert!(
            cache
                .display_ready(&display_key(dest, 4.0, 3.0, 1.0))
                .is_some(),
            "a source in the warm zone must not have its display artifact dropped"
        );
    }
    assert!(cache.display_ready(&hot).is_some());
    assert!(
        cache
            .display_ready(&display_key("d3", 4.0, 3.0, 1.0))
            .is_none(),
        "cold entries are still dropped"
    );
}

#[test]
fn set_working_set_does_not_evict_inflight_or_visible_display() {
    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 20);
    let visible = display_key("d19", 4.0, 3.0, 1.0);
    let inflight = display_key("inf", 4.0, 3.0, 1.0);
    let native = native_from_png(&tiny_png());
    assert!(cache.begin_source("inf".into()));
    assert!(cache.finish_source_inner("inf".into(), Ok(native), None, usize::MAX));
    assert!(cache.begin_display(inflight.clone()));
    cache.set_working_set_inner(
        [visible.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        usize::MAX,
    );
    assert!(cache.display_ready(&visible).is_some());
    assert!(cache.display_contains(&inflight));
    assert_eq!(cache.display_entry_count(), 1 + WARM_DISPLAY_EXTRA);
}

#[test]
fn prefetch_leaves_room_for_the_visible_source() {
    let mut cache = ImageCache::new();
    let hot = display_key("hot", 4.0, 3.0, 1.0);
    let warm = ["w0".to_string(), "w1".to_string(), "w2".to_string()];
    cache.set_working_set_inner([hot], std::iter::empty(), warm.clone(), None, usize::MAX);

    assert!(
        cache.begin_source(warm[0].clone()),
        "the first prefetch has a slot"
    );
    assert!(
        cache.begin_source(warm[1].clone()),
        "the second has one too"
    );
    assert!(
        !cache.begin_source(warm[2].clone()),
        "once prefetch uses up its own slots, no new ones start"
    );
    assert!(
        cache.begin_source("hot".into()),
        "an on-screen source can still start without waiting for the prefetches to finish"
    );
    assert!(
        cache.begin_source("popover".into()),
        "paths that bypass the working set are treated as on-screen"
    );

    assert!(!cache.begin_source("another".into()));
}
