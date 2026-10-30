use super::keys::{cache_key, display_key};
use super::raster::{estimate_display_job, estimate_zoom_job, raster_at, raster_display};
use super::source::{
    decode_data_payload, detect_format, gif_frame_count, is_image_path, markdown_dest,
    parse_retry_after, parse_source, resolve, validate_image_dimensions, validate_svg_dimensions,
    webp_frame_count,
};
use super::{
    Budget, DataEncoding, ImageCache, MAX_DISPLAY_BYTES, MAX_FAILED_ENTRIES, MAX_IN_FLIGHT,
    MAX_RASTER_PIXELS, MAX_SOURCE_BYTES, MIN_DISPLAY_BYTES, Resolved, SOURCE_RETRY_BASE,
    SOURCE_RETRY_MAX, SourceError, SourceKey, SourceMedia, WARM_DISPLAY_EXTRA, WARM_GRACE,
    source_retry_delay,
};
use crate::images::fit::{contain_fit, width_fit};
use crate::media::estimate_bitmap_peak;
use crate::pixels::{capped_dims, snap_css};
use gpui::ImageFormat;
use std::path::PathBuf;

use image::RgbImage;
use std::io::Cursor;
use std::time::{Duration, Instant};

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

fn bitmap_from_png(png: &[u8]) -> SourceMedia {
    parse_source(ImageFormat::Png, png.to_vec()).expect("png source")
}

/// Splice an APP1 EXIF block holding a single Orientation tag right after the
/// JPEG's start-of-image marker.
fn with_exif_orientation(jpeg: Vec<u8>, orientation: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(jpeg.len() + 34);
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&[0xFF, 0xE1, 0x00, 0x22]);
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&[0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&[0x01, 0x00]);
    out.extend_from_slice(&[0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&orientation.to_le_bytes());
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn a_rotated_jpeg_is_measured_and_decoded_upright() {
    let img = image::RgbaImage::from_fn(64, 32, |x, _| {
        if x < 32 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    });
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&image::DynamicImage::ImageRgba8(img))
        .expect("jpeg");
    let media =
        parse_source(ImageFormat::Jpeg, with_exif_orientation(jpeg, 6)).expect("jpeg source");
    assert_eq!(
        media.dims(),
        (32, 64),
        "orientation 6 turns the picture a quarter turn, so the box it reserves swaps"
    );
    let ready = raster_at(&media, (32.0, 64.0), 1.0).expect("raster");
    assert_eq!(
        (ready.px_w, ready.px_h),
        (32, 64),
        "the decoded raster must match the size the layout reserved"
    );
    let pixels = ready.image.as_bytes(0).expect("pixels");
    let at = |x: usize, y: usize| {
        let i = (y * 32 + x) * 4;
        (pixels[i], pixels[i + 2])
    };
    let (top_blue, top_red) = at(25, 5);
    assert!(
        top_red > 200 && top_blue < 60,
        "the left half must land on top: b={top_blue} r={top_red}"
    );
    let (bottom_blue, bottom_red) = at(25, 58);
    assert!(
        bottom_blue > 200 && bottom_red < 60,
        "the right half must land at the bottom: b={bottom_blue} r={bottom_red}"
    );
}

#[test]
fn resident_bytes_counts_the_encoded_source_only() {
    let png = png_with_size(64, 48);
    let media = bitmap_from_png(&png);
    assert_eq!(
        media.resident_bytes(),
        png.len(),
        "a source keeps its encoded bytes, not the 64*48*4 bytes a decode produces"
    );
    assert!(media.resident_bytes() < 64 * 48 * 4);
}

#[test]
fn a_large_canvas_source_keeps_only_its_encoded_bytes() {
    let png = png_with_size(1400, 1400);
    let media = bitmap_from_png(&png);
    assert_eq!(media.dims(), (1400, 1400));
    assert_eq!(media.resident_bytes(), png.len());
    assert!(
        media.resident_bytes() * 100 < 1400 * 1400 * 4,
        "a 7.8 MiB decode must not stay resident: {} bytes",
        media.resident_bytes()
    );
    let key = display_key("big", 400.0, 400.0, 2.0);
    let (slot_w, slot_h) = key.raster_slot();
    let ready = raster_display(&media, &key).expect("raster");
    assert_eq!(
        (ready.px_w, ready.px_h),
        (
            (snap_css(slot_w, 2.0) * 2.0).ceil() as u32,
            (snap_css(slot_h, 2.0) * 2.0).ceil() as u32
        )
    );
}

#[test]
fn raster_at_decodes_the_source_at_the_requested_density() {
    let media = bitmap_from_png(&png_with_size(400, 300));
    let ready = raster_at(&media, (200.0, 150.0), 3.0).expect("raster");
    assert_eq!((ready.px_w, ready.px_h), (600, 450));
    assert!((ready.dpr - 3.0).abs() < 1e-6);
}

#[test]
fn capped_dims_shrinks_an_oversized_source_once() {
    let (w, h) = capped_dims(3000, 3000, MAX_RASTER_PIXELS);
    let pixels = u64::from(w) * u64::from(h);
    assert!(pixels <= MAX_RASTER_PIXELS, "{pixels} pixels exceed budget");
    let rw = w as f32 / 3000.0;
    let rh = h as f32 / 3000.0;
    assert!((rw - rh).abs() < 0.01, "aspect {rw}x{rh} must be preserved");
}

#[test]
fn capped_dims_leaves_sources_within_the_budget_alone() {
    assert_eq!(capped_dims(1000, 800, MAX_RASTER_PIXELS), (1000, 800));
}

#[test]
fn an_oversized_target_keeps_its_css_size_and_drops_density() {
    let media = bitmap_from_png(&png_with_size(2000, 1500));
    let css = (2000.0, 1500.0);
    let ready = raster_at(&media, css, 4.0).expect("raster");
    let pixels = u64::from(ready.px_w) * u64::from(ready.px_h);
    assert!(
        pixels <= MAX_RASTER_PIXELS,
        "{pixels} pixels exceed the raster budget"
    );
    let (w, h) = ready.css_size();
    assert!(
        (w - css.0).abs() <= 1.0,
        "css width {w} must stay at {}, only density may fall",
        css.0
    );
    assert!(
        (h - css.1).abs() <= 1.0,
        "css height {h} must stay at {}, only density may fall",
        css.1
    );
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
    for dest in ["中文名称.png", "图片说明.png", "示意图/a.png", "中文.md"] {
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
        "markdown-editor-rs-img-rel-{}-{}",
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
        "markdown-editor-rs-img-dest-{}-{}",
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
fn a_source_that_fails_to_decode_retires_instead_of_looping() {
    let mut cache = ImageCache::new();
    let key = display_key("broken", 400.0, 300.0, 1.0);
    let job = cache.begin_source("broken".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(bitmap_from_png(&tiny_png())), None, usize::MAX));
    let djob = cache.begin_display(key.clone()).expect("begin display");
    assert!(
        !cache.finish_display_inner(
            djob,
            Err(crate::Error::Image(md_i18n::Key::ImageBadPixels.into())),
            None
        ),
        "a failed raster is not a booked display entry"
    );
    assert!(
        cache.source_media("broken").is_none(),
        "a source whose decode failed must not stay Ready, or every frame schedules the same raster again"
    );
    assert!(cache.source_contains("broken"));
    assert!(cache.failed_sources().contains("broken"));
    assert!(!cache.display_contains(&key));
}

#[test]
fn source_begin_twice_does_not_start_two_fetches() {
    let mut cache = ImageCache::new();
    assert!(cache.begin_source("k".into()).is_some());
    assert!(cache.begin_source("k".into()).is_none());
    assert!(cache.source_contains("k"));
}

#[test]
fn retryable_source_failures_unlock_after_the_backoff_window() {
    let mut cache = ImageCache::new();
    let t0 = Instant::now();
    let err = SourceError::retryable(crate::Error::Image("connect timed out".to_owned().into()));

    let job = cache.begin_source_inner("u".into(), t0).expect("begin");
    cache.finish_source_inner(job, Err(err), None, usize::MAX);
    let soon = t0 + Duration::from_secs(1);
    assert!(!cache.source_retry_due_inner("u", soon));
    assert!(cache.begin_source_inner("u".into(), soon).is_none());
    assert!(cache.source_contains("u"));
    assert!(cache.failed_sources().contains("u"));
    let later = t0 + Duration::from_secs(3600);
    assert!(cache.source_retry_due_inner("u", later));
    let job = cache
        .begin_source_inner("u".into(), later)
        .expect("begin retry");
    assert!(cache.failed_sources().contains("u"));

    let media = bitmap_from_png(&tiny_png());
    cache.finish_source_inner(job, Ok(media), None, usize::MAX);
    assert!(cache.source_media("u").is_some());
    assert!(
        !cache.failed_sources().contains("u"),
        "a successful retry must clear the failure projection, or the fallback text lingers"
    );
    assert_eq!(cache.intrinsic("u"), Some((4, 3)));
}

#[test]
fn retry_backoff_doubles_after_each_failure() {
    let mut cache = ImageCache::new();
    let t0 = Instant::now();
    let err = SourceError::retryable(crate::Error::Image("dns lookup failed".to_owned().into()));

    let job = cache.begin_source_inner("u".into(), t0).expect("begin");
    cache.finish_source_inner(job, Err(err.clone()), None, usize::MAX);
    let first = source_retry_delay(0);
    let t1 = t0 + Duration::from_secs(3600);
    let job = cache
        .begin_source_inner("u".into(), t1)
        .expect("the slot flips once the first window expires");
    cache.finish_source_inner(job, Err(err), None, usize::MAX);
    let t2 = Instant::now();
    let margin = Duration::from_secs(1);
    assert!(
        !cache.source_retry_due_inner("u", t2 + first + margin),
        "the window must double after the second failure"
    );
    assert!(cache.source_retry_due_inner("u", t2 + first * 2 + margin));
}

#[test]
fn retry_after_overrides_the_backoff_window() {
    let mut cache = ImageCache::new();
    let job = cache
        .begin_source_inner("u".into(), Instant::now())
        .expect("begin");
    let before = Instant::now();
    cache.finish_source_inner(
        job,
        Err(SourceError::Retryable {
            err: crate::Error::Image("too many requests".to_owned().into()),
            retry_after: Some(Duration::from_secs(8)),
        }),
        None,
        usize::MAX,
    );
    let after = Instant::now();
    assert!(
        !cache.source_retry_due_inner("u", before + Duration::from_secs(7)),
        "must not admit before the server window expires"
    );
    assert!(
        cache.source_retry_due_inner("u", after + Duration::from_secs(8)),
        "must admit once the server window expires"
    );
}

#[test]
fn parse_retry_after_accepts_seconds_and_clamps() {
    use gpui::http_client::http::{HeaderMap, HeaderName, HeaderValue};

    fn header(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("retry-after"),
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    assert_eq!(
        parse_retry_after(&header("8")),
        Some(Duration::from_secs(8)),
        "delta-seconds accepted as-is"
    );
    assert_eq!(
        parse_retry_after(&header("1")),
        Some(SOURCE_RETRY_BASE),
        "too small clamps back to the floor"
    );
    assert_eq!(
        parse_retry_after(&header("3600")),
        Some(SOURCE_RETRY_MAX),
        "too large clamps to the ceiling"
    );
    assert_eq!(
        parse_retry_after(&header("Wed, 21 Oct 2026 07:28:00 GMT")),
        None,
        "an HTTP-date hands back to exponential backoff"
    );
    assert_eq!(
        parse_retry_after(&header("soon")),
        None,
        "a garbage value hands back to exponential backoff"
    );
    assert_eq!(
        parse_retry_after(&HeaderMap::new()),
        None,
        "a missing header hands back to exponential backoff"
    );
}

#[test]
fn deterministic_source_failures_never_unlock() {
    let mut cache = ImageCache::new();
    let much_later = Instant::now() + Duration::from_secs(86_400);
    let job = cache
        .begin_source_inner("k".into(), Instant::now())
        .expect("begin");
    cache.finish_source_inner(
        job,
        Err(SourceError::Fatal(crate::Error::Image(
            md_i18n::Key::ImageBadPath.into(),
        ))),
        None,
        usize::MAX,
    );
    assert!(cache.source_contains("k"));
    assert!(!cache.source_retry_due_inner("k", much_later));
    assert!(
        cache.begin_source_inner("k".into(), much_later).is_none(),
        "a deterministic failure must not flip the slot for retry"
    );
    assert!(cache.failed_sources().contains("k"));
}

#[test]
fn failed_sources_are_bounded_and_evict_the_oldest_cold_key() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let fatal = || SourceError::Fatal(crate::Error::Image(md_i18n::Key::ImageBadPath.into()));
    for i in 0..=MAX_FAILED_ENTRIES {
        let job = cache.begin_source_inner(format!("f{i}"), t).expect("begin");
        cache.finish_source_inner(job, Err(fatal()), None, usize::MAX);
    }
    assert!(
        !cache.source_contains("f0"),
        "after overflow the oldest failure key should be evicted"
    );
    assert!(
        !cache.failed_sources().contains("f0"),
        "the failure projection must be cleared together with the slot"
    );
    assert!(cache.source_contains(&format!("f{MAX_FAILED_ENTRIES}")));
    let last = format!("f{MAX_FAILED_ENTRIES}");
    assert!(cache.failed_sources().contains(last.as_str()));
    assert_eq!(cache.failed_entry_count(), MAX_FAILED_ENTRIES);
}

#[test]
fn visible_failed_sources_survive_the_cap() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let fatal = || SourceError::Fatal(crate::Error::Image(md_i18n::Key::ImageBadPath.into()));
    let hot = display_key("f0", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [hot],
        ["f0".to_string()],
        std::iter::empty(),
        None,
        usize::MAX,
    );
    for i in 0..=MAX_FAILED_ENTRIES {
        let job = cache.begin_source_inner(format!("f{i}"), t).expect("begin");
        cache.finish_source_inner(job, Err(fatal()), None, usize::MAX);
    }
    assert!(
        cache.source_contains("f0"),
        "a visible failure key must not be evicted"
    );
    assert!(
        !cache.source_contains("f1"),
        "it should evict the oldest invisible key"
    );
    assert!(cache.source_contains(&format!("f{MAX_FAILED_ENTRIES}")));
}

#[test]
fn recovered_sources_leave_the_failure_lru() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let err = SourceError::retryable(crate::Error::Image("down".to_owned().into()));
    let job = cache.begin_source_inner("u".into(), t).expect("begin");
    cache.finish_source_inner(job, Err(err), None, usize::MAX);
    assert_eq!(cache.failed_entry_count(), 1);
    let later = t + Duration::from_secs(3600);
    let job = cache
        .begin_source_inner("u".into(), later)
        .expect("begin retry");
    let media = bitmap_from_png(&tiny_png());
    cache.finish_source_inner(job, Ok(media), None, usize::MAX);
    assert_eq!(
        cache.failed_entry_count(),
        0,
        "a successful retry leaves no ghost entry in the failure LRU"
    );
}

#[test]
fn retrying_source_survives_failure_lru_eviction() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let fatal = || SourceError::Fatal(crate::Error::Image(md_i18n::Key::ImageBadPath.into()));
    let retryable = || SourceError::retryable(crate::Error::Image("down".to_owned().into()));

    let old_job = cache.begin_source_inner("retry".into(), t).expect("begin");
    cache.finish_source_inner(old_job, Err(retryable()), None, usize::MAX);
    let later = t + Duration::from_secs(3600);
    let retry_job = cache
        .begin_source_inner("retry".into(), later)
        .expect("begin retry");

    for i in 0..MAX_FAILED_ENTRIES - 1 {
        let key = format!("bad-{i}");
        let job = cache.begin_source_inner(key, later).expect("begin");
        cache.finish_source_inner(job, Err(fatal()), None, usize::MAX);
    }
    let job = cache
        .begin_source_inner("overflow".into(), later)
        .expect("begin");
    cache.finish_source_inner(job, Err(fatal()), None, usize::MAX);
    assert!(
        cache.source_contains("retry"),
        "a slot mid-retry must not be evicted by the failure LRU"
    );
    assert_eq!(
        cache.failed_entry_count(),
        MAX_FAILED_ENTRIES,
        "after evicting one cold failure key, the failure LRU lands exactly on its cap"
    );

    let media = bitmap_from_png(&tiny_png());
    assert!(
        cache.finish_source_inner(retry_job, Ok(media), None, usize::MAX),
        "a completion while the retry slot is alive must be booked"
    );
    assert!(cache.source_media("retry").is_some());
    for n in 0..MAX_IN_FLIGHT {
        assert!(
            cache
                .begin_source_inner(format!("fresh-{n}"), later)
                .is_some(),
            "eviction must not leak the in-flight quota: the {n}th new key cannot enter"
        );
    }
}

#[test]
fn in_flight_slots_never_leak_a_permit_through_eviction() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let fatal = || SourceError::Fatal(crate::Error::Image(md_i18n::Key::ImageBadPath.into()));

    let old_job = cache
        .begin_source_inner("inflight".into(), t)
        .expect("begin");
    cache
        .failed_lru
        .push_back(SourceKey::new("inflight".to_owned()));
    for i in 0..MAX_FAILED_ENTRIES {
        let key = format!("bad-{i}");
        let job = cache.begin_source_inner(key, t).expect("begin");
        cache.finish_source_inner(job, Err(fatal()), None, usize::MAX);
    }
    assert!(
        cache.source_contains("inflight"),
        "eviction must not pick an in-flight slot"
    );

    cache.finish_source_inner(old_job, Err(fatal()), None, usize::MAX);
    for n in 0..MAX_IN_FLIGHT {
        assert!(
            cache.begin_source_inner(format!("fresh-{n}"), t).is_some(),
            "deleting an in-flight slot must return its quota: the {n}th new key cannot enter"
        );
    }
}

#[test]
fn evicted_then_re_entered_key_rejects_the_old_jobs_result() {
    let mut cache = ImageCache::new();
    let t = Instant::now();
    let fatal = || SourceError::Fatal(crate::Error::Image(md_i18n::Key::ImageBadPath.into()));

    let old = cache.begin_source_inner("u".into(), t).expect("begin");
    cache.remove_source(&SourceKey::new("u".to_owned()), None);
    assert!(
        !cache.source_contains("u"),
        "precondition: the slot was deleted"
    );
    let new = cache.begin_source_inner("u".into(), t).expect("re-entry");

    let retryable = SourceError::retryable(crate::Error::Image("down".to_owned().into()));
    assert!(
        !cache.finish_source_inner(old, Err(retryable), None, usize::MAX),
        "after deleting and re-entering a slot, a late result from the old job must not displace the new job"
    );
    assert!(
        cache.finish_source_inner(new, Err(fatal()), None, usize::MAX),
        "the new job's failure must be booked"
    );
    assert!(cache.source_error("u").is_some());
}

#[test]
fn display_drop_keeps_source_ready() {
    let png = tiny_png();
    let media = bitmap_from_png(&png);
    let mut cache = ImageCache::new();
    let job = cache.begin_source("k".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, usize::MAX));
    assert_eq!(cache.intrinsic("k"), Some((4, 3)));
    let key = display_key("k", 4.0, 3.0, 1.0);
    let djob = cache.begin_display(key.clone()).expect("begin display");
    let ready = raster_display(&media, &key).expect("raster");
    assert!(cache.finish_display_inner(djob, Ok(ready), None));
    assert!(cache.display_ready(&key).is_some());
    cache.drop_display_keep_source(&key);
    assert!(cache.display_ready(&key).is_none());
    assert_eq!(cache.intrinsic("k"), Some((4, 3)));
    assert!(cache.source_media("k").is_some());
}

#[test]
fn source_lru_evicts_derived_displays_and_retries_budget_pressure() {
    let media = bitmap_from_png(&tiny_png());
    let source_bytes = media.resident_bytes();
    let mut cache = ImageCache::new();

    let a = display_key("a", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        source_bytes,
    );
    let job = cache.begin_source("a".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, source_bytes));
    let djob = cache.begin_display(a.clone()).expect("begin display");
    let display = raster_display(&media, &a).expect("display");
    assert!(cache.finish_display_inner(djob, Ok(display), None));

    let b = display_key("b", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        source_bytes,
    );
    let job = cache.begin_source("b".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, source_bytes));
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
    let job = cache.begin_source("c".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media), None, source_bytes));
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
    let media = bitmap_from_png(&tiny_png());
    let n = media.resident_bytes();
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
    let job = cache.begin_source("a".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, budget));

    let b = display_key("b", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone(), b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    let job = cache.begin_source("b".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, budget));
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
        "a visible OverBudget must not repeatedly clear slots when nothing fits"
    );

    cache.set_working_set_inner([b], std::iter::empty(), std::iter::empty(), None, budget);
    assert!(
        !cache.source_contains("b"),
        "once room is made, the OverBudget slots should be cleared"
    );
    let job = cache.begin_source("b".into()).expect("begin retry");
    assert!(cache.finish_source_inner(job, Ok(media), None, budget));
    assert_eq!(cache.intrinsic("b"), Some((4, 3)));
    assert!(
        !cache.source_contains("a"),
        "booking a retry should evict the offscreen a"
    );
}

#[test]
fn image_larger_than_the_whole_budget_never_retries() {
    let media = bitmap_from_png(&png_with_size(6, 5));
    let budget = media.resident_bytes().saturating_sub(1);
    let mut cache = ImageCache::new();
    let k = display_key("k", 6.0, 5.0, 1.0);
    cache.set_working_set_inner(
        [k.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    let job = cache.begin_source("k".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media), None, budget));
    assert!(cache.source_error("k").is_some());
    cache.set_working_set_inner([k], std::iter::empty(), std::iter::empty(), None, budget);
    assert!(cache.source_contains("k"));
}

#[test]
fn raster_downscales_from_source_pixels() {
    let png = png_with_size(400, 300);
    let media = bitmap_from_png(&png);
    let key = display_key("k", 256.0, 192.0, 1.0);
    let ready = raster_display(&media, &key).expect("raster");
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

fn gif_with_canvas_and_frames(width: u32, height: u32, frames: usize) -> Vec<u8> {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&(width as u16).to_le_bytes());
    gif.extend_from_slice(&(height as u16).to_le_bytes());
    gif.extend_from_slice(&[0, 0, 0]);
    for _ in 0..frames {
        gif.push(0x2C);
        gif.extend_from_slice(&[0, 0, 0, 0]);
        gif.extend_from_slice(&(width as u16).to_le_bytes());
        gif.extend_from_slice(&(height as u16).to_le_bytes());
        gif.push(0);
        gif.push(2);
        gif.extend_from_slice(&[1, 0, 0]);
    }
    gif.push(b';');
    gif
}

fn animated_webp(canvas: (u32, u32), frames: usize) -> Vec<u8> {
    fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
        if payload.len() & 1 == 1 {
            out.push(0);
        }
    }
    let img = image::RgbaImage::from_pixel(100, 80, image::Rgba([1, 2, 3, 255]));
    let mut raw = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut raw),
            image::ImageFormat::WebP,
        )
        .expect("webp");
    let pos = raw.windows(4).position(|w| w == b"VP8L").expect("VP8L");
    let size =
        u32::from_le_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]) as usize;
    let chunk = raw[pos..pos + 8 + size].to_vec();

    let mut chunks = Vec::new();
    let mut vp8x = vec![0x02, 0, 0, 0];
    vp8x.extend_from_slice(&(canvas.0 - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(canvas.1 - 1).to_le_bytes()[..3]);
    push_chunk(&mut chunks, b"VP8X", &vp8x);
    push_chunk(&mut chunks, b"ANIM", &[0; 6]);
    for _ in 0..frames {
        let mut anmf = Vec::new();
        anmf.extend_from_slice(&[0; 6]);
        anmf.extend_from_slice(&99u32.to_le_bytes()[..3]);
        anmf.extend_from_slice(&79u32.to_le_bytes()[..3]);
        anmf.extend_from_slice(&[0; 4]);
        anmf.extend_from_slice(&chunk);
        push_chunk(&mut chunks, b"ANMF", &anmf);
    }
    let riff_size = 4 + chunks.len();
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&(riff_size as u32).to_le_bytes());
    webp.extend_from_slice(b"WEBP");
    webp.extend_from_slice(&chunks);
    webp
}

#[test]
fn multiframe_gifs_over_the_decoded_budget_are_rejected_before_decode() {
    let bomb = gif_with_canvas_and_frames(4000, 4000, 3);
    let err = validate_image_dimensions(ImageFormat::Gif, &bomb)
        .expect_err("three 4000×4000 frames exceed the cumulative decode budget and must be refused before decoding");
    assert!(matches!(err, crate::Error::Image(_)));
    let per_frame = 1400usize * 1400 * 4;
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &gif_with_canvas_and_frames(1400, 1400, 5))
            .is_err(),
        "five frames at {} bytes exceed the animation budget",
        per_frame * 5
    );
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &gif_with_canvas_and_frames(1400, 1400, 2))
            .is_ok()
    );
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &gif_with_canvas_and_frames(4, 4, 3)).is_ok()
    );
}

#[test]
fn gifs_with_more_frames_than_the_animation_cap_are_rejected() {
    let bomb = gif_with_canvas_and_frames(4, 4, 513);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &bomb).is_err(),
        "513 frames must be refused by the frame cap"
    );
    let capped = gif_with_canvas_and_frames(4, 4, 512);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &capped).is_ok(),
        "512 frames, within the cap, must pass"
    );
}

#[test]
fn animations_get_a_tighter_decoded_budget_than_stills() {
    let still = gif_with_canvas_and_frames(4200, 4200, 1);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &still).is_ok(),
        "a single ~70 MiB frame still fits the still budget"
    );
    let animated = gif_with_canvas_and_frames(1024, 1024, 9);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &animated).is_err(),
        "nine 4 MiB frames exceed the 32 MiB animation budget"
    );
    let within = gif_with_canvas_and_frames(1024, 1024, 8);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &within).is_ok(),
        "eight 4 MiB frames land exactly on the 32 MiB animation budget"
    );
}

#[test]
fn single_frames_over_the_byte_budget_are_rejected_before_decode() {
    let bomb = png_with_header_only(6000, 7000);
    let err = validate_image_dimensions(ImageFormat::Png, &bomb)
        .expect_err("a 42M-pixel single frame exceeds the decode budget");
    assert!(matches!(err, crate::Error::Image(_)));
}

#[test]
fn gif_frame_counts_match_real_encoder_output() {
    let mut buf = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut buf);
        encoder
            .encode_frame(image::Frame::new(image::RgbaImage::new(4, 3)))
            .expect("frame");
        encoder
            .encode_frame(image::Frame::new(image::RgbaImage::new(4, 3)))
            .expect("frame");
    }
    assert_eq!(gif_frame_count(&buf), Some(2));
    assert!(validate_image_dimensions(ImageFormat::Gif, &buf).is_ok());
    assert_eq!(gif_frame_count(&buf[..buf.len() - 1]), None);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &buf[..buf.len() - 1]).is_err(),
        "a truncated chunk structure must be rejected by the precheck"
    );
}

#[test]
fn webp_frame_counts_read_anmf_chunks() {
    let animated = animated_webp((4000, 4000), 3);
    let err = validate_image_dimensions(ImageFormat::Webp, &animated)
        .expect_err("three 4000×4000 WebP frames exceed the cumulative decode budget");
    assert!(matches!(err, crate::Error::Image(_)));
    assert_eq!(webp_frame_count(&animated), Some(3));
    let mut truncated = animated_webp((4000, 4000), 3);
    truncated.truncate(truncated.len() - 5);
    assert_eq!(webp_frame_count(&truncated), None);
    let img = image::RgbaImage::from_pixel(100, 80, image::Rgba([9, 8, 7, 255]));
    let mut still = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut still),
            image::ImageFormat::WebP,
        )
        .expect("webp");
    assert_eq!(webp_frame_count(&still), Some(1));
    assert!(validate_image_dimensions(ImageFormat::Webp, &still).is_ok());
    assert!(
        validate_image_dimensions(ImageFormat::Webp, &animated_webp((100, 80), 3)).is_ok(),
        "an animation within the frame cap and the cumulative budget passes as usual"
    );
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
            "a png truncated to {n} bytes must be refused"
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
        .expect_err("a 65535^2 pixel claim must be refused on size");
    assert!(matches!(err, crate::Error::Image(_)));

    let zero = png_with_header_only(0, 3);
    let err = validate_image_dimensions(ImageFormat::Png, &zero)
        .expect_err("a 0-width claim must be refused");
    assert!(matches!(err, crate::Error::Image(_)));
}

#[test]
fn jpeg_pixel_declarations_above_the_cap_are_rejected() {
    let bomb = real_jpeg_with_declared(9, 7, (40000, 40000));
    assert!(
        validate_image_dimensions(ImageFormat::Jpeg, &bomb).is_err(),
        "an SOF claiming 40000^2 pixels must be refused"
    );
    let honest = real_jpeg_with_declared(9, 7, (9, 7));
    assert!(
        validate_image_dimensions(ImageFormat::Jpeg, &honest).is_ok(),
        "an untampered baseline JPEG must pass as usual"
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
        "100k nested <g> layers must be rejected by the depth prescan before parsing"
    );
    assert!(
        detect_format(deep.as_bytes()).is_none(),
        "format probing must not identify it as SVG either (same usvg call)"
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
        "tag text inside comments/CDATA/attribute values must not count toward depth"
    );
}

#[test]
fn truncated_png_bodies_pass_the_guard_and_fail_in_the_decoder() {
    let full = png_with_size(4, 3);
    let truncated = &full[..full.len() - 10];
    assert!(validate_image_dimensions(ImageFormat::Png, truncated).is_ok());
    assert!(
        image::load_from_memory_with_format(truncated, image::ImageFormat::Png).is_err(),
        "a body with the tail cut off must fail in the decoder"
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
        "JPEG bytes labelled PNG must be rejected by the PNG decoder"
    );
}

fn fill_display_ready(cache: &mut ImageCache, n: usize) {
    let media = bitmap_from_png(&tiny_png());
    for i in 0..n {
        let dest = format!("d{i}");
        let key = display_key(&dest, 4.0, 3.0, 1.0);
        let job = cache.begin_source(dest).expect("begin");
        assert!(cache.finish_source_inner(job, Ok(media.clone()), None, usize::MAX));
        let djob = cache.begin_display(key.clone()).expect("begin display");
        let display = raster_display(&media, &key).expect("display");
        assert!(cache.finish_display_inner(djob, Ok(display), None));
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
    let each = bitmap_from_png(&tiny_png()).resident_bytes();
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
        "this test's premise is that an eviction really happened"
    );
    for dest in &warm {
        assert!(
            cache.source_contains(dest),
            "a warm source must not be dropped while cold entries remain"
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
            "a source in the warm zone must not lose its display-layer product"
        );
    }
    assert!(cache.display_ready(&hot).is_some());
    assert!(
        cache
            .display_ready(&display_key("d3", 4.0, 3.0, 1.0))
            .is_none(),
        "cold entries still get dropped"
    );
}

#[test]
fn set_working_set_does_not_evict_inflight_or_visible_display() {
    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 20);
    let visible = display_key("d19", 4.0, 3.0, 1.0);
    let inflight = display_key("inf", 4.0, 3.0, 1.0);
    let media = bitmap_from_png(&tiny_png());
    let job = cache.begin_source("inf".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media), None, usize::MAX));
    assert!(cache.begin_display(inflight.clone()).is_some());
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
        cache.begin_source(warm[0].clone()).is_some(),
        "the first prefetch has a slot"
    );
    assert!(
        cache.begin_source(warm[1].clone()).is_some(),
        "so does the second"
    );
    assert!(
        cache.begin_source(warm[2].clone()).is_none(),
        "once prefetch uses up its own quota, it opens no more"
    );
    assert!(
        cache.begin_source("hot".into()).is_some(),
        "an on-screen source can still start, no need to wait for prefetch"
    );
    assert!(
        cache.begin_source("popover".into()).is_some(),
        "a path that skips the working set is treated as on-screen"
    );
    assert!(cache.begin_source("another".into()).is_none());
}

#[test]
fn the_account_reports_the_bytes_the_cache_actually_holds() {
    let media = bitmap_from_png(&png_with_size(40, 30));
    let source_bytes = media.resident_bytes();
    let mut cache = ImageCache::new();
    let key = display_key("k", 40.0, 30.0, 1.0);
    cache.set_working_set_inner(
        [key.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        48,
    );
    let job = cache.begin_source("k".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(media.clone()), None, usize::MAX));

    let account = cache.account();
    assert_eq!(account.source_bytes, source_bytes);
    assert_eq!(account.source_entries, 1);
    assert_eq!(account.display_bytes, 0);
    assert_eq!(account.display_entries, 0);
    assert_eq!(account.media_bytes(), source_bytes);

    let djob = cache.begin_display(key.clone()).expect("display");
    let ready = raster_display(&media, &key).expect("raster");
    let display_bytes = ready.bytes;
    assert!(cache.finish_display_inner(djob, Ok(ready), None));

    let account = cache.account();
    assert_eq!(account.display_bytes, display_bytes);
    assert_eq!(account.display_entries, 1);
    assert_eq!(account.media_bytes(), source_bytes + display_bytes);

    cache.drop_display_keep_source(&key);
    let account = cache.account();
    assert_eq!(account.display_bytes, 0);
    assert_eq!(account.display_entries, 0);
    assert_eq!(account.media_bytes(), source_bytes);
}

#[test]
fn a_viewport_budget_trims_the_display_window() {
    let budget = Budget::for_viewport(804.0, 620.0, 1.25);
    assert_eq!(budget.display_entries, 48);
    assert!(budget.display_bytes < MAX_DISPLAY_BYTES);
    assert!(budget.source_bytes < MAX_SOURCE_BYTES);

    let mut cache = ImageCache::new();
    fill_display_ready(&mut cache, 60);
    cache.set_budget_inner(budget, None);
    assert!(cache.display_entry_count() <= 48);
}

#[test]
fn a_viewport_budget_grows_with_the_window() {
    let small = Budget::for_viewport(400.0, 350.0, 1.0);
    let large = Budget::for_viewport(1920.0, 1080.0, 1.0);
    let huge = Budget::for_viewport(3840.0, 2160.0, 2.0);
    assert_eq!(small.display_bytes, MIN_DISPLAY_BYTES);
    assert!(large.display_bytes > small.display_bytes);
    assert!(huge.source_bytes > small.source_bytes);
    assert!(huge.display_bytes > large.display_bytes);
}

#[test]
fn an_estimate_counts_the_pixels_the_file_decodes_to_not_the_capped_ones() {
    let media = SourceMedia::Bitmap {
        bytes: std::sync::Arc::new(Vec::new()),
        format: ImageFormat::Png,
        width: 2048,
        height: 2048,
        decoded: (4096, 4096),
        lease: None,
    };
    assert_eq!(media.dims(), (2048, 2048));
    assert_eq!(media.decoded_dims(), (4096, 4096));
    let key = display_key("k", 800.0, 800.0, 2.0);
    assert_eq!(
        estimate_display_job(&media, &key),
        estimate_bitmap_peak(4096, 4096, 1600, 1600),
        "the decode allocates the file's pixels, so the estimate must start from them"
    );
}

#[test]
fn a_display_estimate_names_the_pixels_the_raster_allocates() {
    let media = bitmap_from_png(&png_with_size(600, 400));
    let key = display_key("k", 300.0, 200.0, 2.0);
    let ready = raster_display(&media, &key).expect("raster");
    let estimate = estimate_display_job(&media, &key);
    assert_eq!(
        estimate,
        estimate_bitmap_peak(600, 400, ready.px_w, ready.px_h),
        "the scheduler must size the job from the source and the slot, without decoding"
    );
    assert!(
        estimate > ready.bytes,
        "the peak covers the decoded source and the resize workspace as well as the output"
    );
}

#[test]
fn a_zoom_estimate_follows_the_target_into_the_pixel_cap() {
    let media = bitmap_from_png(&png_with_size(2000, 1500));
    let css = (2000.0, 1500.0);
    let ready = raster_at(&media, css, 4.0).expect("raster");
    let estimate = estimate_zoom_job(&media, css, 4.0);
    assert_eq!(
        estimate,
        estimate_bitmap_peak(2000, 1500, ready.px_w, ready.px_h),
        "a capped target must be what the estimate asks for, not the density the caller wanted"
    );
    let uncapped = estimate_bitmap_peak(2000, 1500, 8000, 6000);
    assert!(
        estimate < uncapped,
        "an uncapped target would ask for {uncapped} bytes"
    );
}

fn leased_display(
    cx: &gpui::App,
    media: &SourceMedia,
    key: &super::DisplayKey,
) -> crate::pixels::ReadyImage {
    let permit = crate::media::MediaCoordinator::admit(cx, crate::media::JobKind::Display, 1 << 20)
        .expect("admit a display job");
    let image = raster_display(media, key).expect("raster");
    let bytes = image.bytes;
    image.with_lease(permit.into_lease(bytes))
}

#[gpui::test]
fn app_pressure_evicts_past_the_display_slice_and_a_snapshot_keeps_it_charged(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 10);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let media = bitmap_from_png(&png_with_size(16, 16));

        let first = display_key("a", 16.0, 16.0, 1.0);
        let token = cache.begin_display(first.clone()).expect("begin a");
        let image = leased_display(cx, &media, &first);
        assert_eq!(image.bytes, 1024);
        assert!(cache.finish_display(token, Ok(image), cx));
        assert_eq!(crate::media::MediaCoordinator::app_resident(cx), 1024);

        let snapshot = cache.ready_snapshot();

        let second = display_key("b", 16.0, 16.0, 1.0);
        let token = cache.begin_display(second.clone()).expect("begin b");
        let image = leased_display(cx, &media, &second);
        assert!(cache.finish_display(token, Ok(image), cx));

        assert_eq!(
            cache.display_entry_count(),
            0,
            "the resident ceiling is exceeded, so both displays go before either slice is refilled"
        );
        assert_eq!(
            crate::media::MediaCoordinator::app_resident(cx),
            1024,
            "the snapshot still holds one image, so its pixels stay charged after the entry is gone"
        );

        drop(snapshot);
        assert_eq!(
            crate::media::MediaCoordinator::app_resident(cx),
            0,
            "the last clone of each image returns its lease"
        );
    });
}

#[gpui::test]
fn a_late_display_result_is_dropped_with_its_lease(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        let media = bitmap_from_png(&png_with_size(16, 16));
        let key = display_key("a", 16.0, 16.0, 1.0);

        let token = cache.begin_display(key.clone()).expect("begin");
        let image = leased_display(cx, &media, &key);
        assert!(cache.finish_display(token.clone(), Ok(image), cx));
        assert_eq!(crate::media::MediaCoordinator::app_resident(cx), 1024);

        let late = leased_display(cx, &media, &key);
        assert!(
            !cache.finish_display(token, Ok(late), cx),
            "the slot already holds a newer result"
        );
        assert_eq!(
            crate::media::MediaCoordinator::app_resident(cx),
            1024,
            "the late pixels must give their lease back instead of double charging"
        );
    });
}

#[gpui::test]
fn a_source_lease_covers_its_encoded_bytes_until_the_cache_lets_go(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let media = bitmap_from_png(&png_with_size(16, 16));
        let resident = media.resident_bytes();
        let permit =
            crate::media::MediaCoordinator::admit(cx, crate::media::JobKind::Source, 1 << 20)
                .expect("admit a source job");
        let media = media.with_lease(permit.into_lease(resident));
        let token = cache.begin_source("a".into()).expect("begin source");
        assert!(cache.finish_source(token, Ok(media.clone()), cx));
        assert_eq!(crate::media::MediaCoordinator::app_resident(cx), resident);

        drop(media);
        assert_eq!(
            crate::media::MediaCoordinator::app_resident(cx),
            resident,
            "the cache still holds the source, so its bytes stay charged"
        );

        cache.clear(cx);
        assert_eq!(crate::media::MediaCoordinator::app_resident(cx), 0);
    });
}

fn warm_copy(cx: &gpui::App, image: &crate::pixels::ReadyImage) -> crate::media::WarmImage {
    let codec = crate::media::WarmCodec::Deflate;
    let bytes = crate::media::encode(image, codec).expect("encode");
    let len = bytes.len();
    let permit = crate::media::MediaCoordinator::admit(cx, crate::media::JobKind::Cool, 1 << 20)
        .expect("admit a cool job");
    crate::media::WarmImage::new(bytes, codec, image.px_w, image.px_h, image.dpr)
        .with_lease(permit.into_lease(len))
}

fn ready_in(cache: &mut ImageCache, cx: &mut gpui::App, dest: &str) -> super::DisplayKey {
    let media = bitmap_from_png(&png_with_size(16, 16));
    let key = display_key(dest, 16.0, 16.0, 1.0);
    let token = cache.begin_display(key.clone()).expect("begin");
    let image = leased_display(cx, &media, &key);
    assert!(cache.finish_display(token, Ok(image), cx));
    key
}

#[gpui::test]
fn an_entry_that_stays_off_screen_is_offered_for_compression(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");

        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );
        assert!(
            cache.coolable_at(Instant::now(), false).is_empty(),
            "an entry that has just left the screen is not worth an encode yet"
        );
        let later = Instant::now() + WARM_GRACE + Duration::from_millis(1);
        assert_eq!(cache.coolable_at(later, false), vec![key.clone()]);

        cache.set_working_set([key], std::iter::empty(), std::iter::empty(), cx);
        assert!(
            cache.coolable_at(later, false).is_empty(),
            "coming back to the screen must clear the stamp, not just pause it"
        );
    });
}

#[gpui::test]
fn compressing_an_off_screen_entry_replaces_its_pixels_with_bytes(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );

        let (token, image) = cache.begin_cool(&key).expect("cool");
        let warm = warm_copy(cx, &image);
        assert!(warm.resident_bytes() < image.bytes, "the fixture must compress");
        drop(image);
        assert!(cache.finish_cool(token, warm.clone(), cx));

        assert_eq!(cache.account().display_bytes, 0);
        assert_eq!(cache.account().warm_bytes, warm.resident_bytes());
        assert_eq!(cache.account().warm_entries, 1);
        assert!(
            cache.ready_snapshot().get(&key).is_none(),
            "a warm entry has no pixels, so nothing may paint it"
        );
        assert!(
            !cache.display_contains(&key),
            "the entry is not paintable, so a job has to be able to start for it"
        );
        assert_eq!(
            crate::media::MediaCoordinator::app_resident(cx),
            warm.resident_bytes(),
            "the pixels' lease goes and the compressed bytes take its place"
        );
    });
}

#[gpui::test]
fn a_compression_that_barely_shrank_is_refused_and_not_retried(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );

        let (token, image) = cache.begin_cool(&key).expect("cool");
        let no_gain = crate::media::WarmImage::new(
            vec![0; image.bytes],
            crate::media::WarmCodec::Deflate,
            image.px_w,
            image.px_h,
            image.dpr,
        );
        assert!(
            !cache.finish_cool(token, no_gain, cx),
            "a payload that is not smaller than the pixels is not worth keeping"
        );
        assert_eq!(cache.account().display_bytes, 1024);
        assert_eq!(cache.account().warm_bytes, 0);
        let later = Instant::now() + WARM_GRACE + Duration::from_millis(1);
        assert!(
            cache.coolable_at(later, false).is_empty(),
            "a refused entry must not be re-encoded on every update"
        );
    });
}

#[gpui::test]
fn an_entry_that_came_back_while_cooling_keeps_its_pixels(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );

        let (token, image) = cache.begin_cool(&key).expect("cool");
        let warm = warm_copy(cx, &image);
        cache.set_working_set([key.clone()], std::iter::empty(), std::iter::empty(), cx);
        assert!(
            !cache.finish_cool(token, warm, cx),
            "an entry that is on screen again must not be swapped for its compressed form"
        );
        assert!(cache.display_ready(&key).is_some());
        assert_eq!(cache.account().warm_bytes, 0);
    });
}

#[gpui::test]
fn a_warm_entry_inflates_back_into_pixels(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );
        let (token, image) = cache.begin_cool(&key).expect("cool");
        let warm = warm_copy(cx, &image);
        assert!(cache.finish_cool(token, warm.clone(), cx));

        let (token, stored) = cache.begin_thaw(&key).expect("thaw");
        assert!(
            !cache.display_contains(&key),
            "an entry being inflated has no pixels to paint either"
        );
        let restored = crate::media::decode(&stored).expect("inflate");
        assert!(cache.finish_thaw(token, Some(restored), cx));

        assert_eq!(cache.account().warm_bytes, 0);
        assert_eq!(cache.account().display_bytes, 1024);
        assert_eq!(cache.warm_entry_count(), 0);
        assert!(cache.display_ready(&key).is_some());
    });
}

#[gpui::test]
fn a_thaw_that_cannot_be_admitted_leaves_the_entry_warm(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );
        let (token, image) = cache.begin_cool(&key).expect("cool");
        let warm = warm_copy(cx, &image);
        assert!(cache.finish_cool(token, warm.clone(), cx));

        let (token, _stored) = cache.begin_thaw(&key).expect("thaw");
        cache.cancel_thaw(token);
        assert_eq!(cache.account().warm_bytes, warm.resident_bytes());
        assert_eq!(cache.warm_entry_count(), 1);
    });
}

#[gpui::test]
fn a_payload_that_will_not_inflate_is_dropped_so_the_next_frame_rasterises(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );
        let (token, image) = cache.begin_cool(&key).expect("cool");
        assert!(cache.finish_cool(token, warm_copy(cx, &image), cx));

        let (token, _stored) = cache.begin_thaw(&key).expect("thaw");
        assert!(!cache.finish_thaw(token, None, cx));
        assert_eq!(cache.account().warm_bytes, 0);
        assert_eq!(
            cache.display_entry_count(),
            0,
            "a corrupt payload says nothing about the source, so the entry goes and is rebuilt"
        );
    });
}

#[gpui::test]
fn shrinking_the_budget_evicts_warm_entries_and_their_bytes(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 20, 1 << 20);
        let mut cache = ImageCache::new();
        cache.set_budget(
            Budget {
                display_bytes: 1 << 20,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        let key = ready_in(&mut cache, cx, "a");
        cache.set_working_set(
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            cx,
        );
        let (token, image) = cache.begin_cool(&key).expect("cool");
        assert!(cache.finish_cool(token, warm_copy(cx, &image), cx));
        assert_eq!(cache.warm_entry_count(), 1);

        cache.set_budget(
            Budget {
                display_bytes: 0,
                display_entries: 8,
                source_bytes: 1 << 20,
            },
            cx,
        );
        assert_eq!(cache.warm_entry_count(), 0);
        assert_eq!(cache.account().warm_bytes, 0);
    });
}

#[gpui::test]
fn cooling_keeps_more_entries_within_the_same_budget(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::media::MediaCoordinator::install_with(cx, 1 << 24, 1 << 24);
        let roomy = Budget {
            display_bytes: 1 << 20,
            display_entries: 64,
            source_bytes: 1 << 20,
        };
        let tight = Budget {
            display_bytes: 8 * 1024,
            display_entries: 64,
            source_bytes: 1 << 20,
        };

        let mut hot_only = ImageCache::new();
        hot_only.set_budget(roomy, cx);
        let mut cooled = ImageCache::new();
        cooled.set_budget(roomy, cx);
        for index in 0..12 {
            let dest = format!("c{index}");
            ready_in(&mut hot_only, cx, &dest);
            ready_in(&mut cooled, cx, &dest);
        }
        let cooled_keys: Vec<super::DisplayKey> = (0..8)
            .map(|index| display_key(&format!("c{index}"), 16.0, 16.0, 1.0))
            .collect();
        for key in &cooled_keys {
            let (token, image) = cooled.begin_cool(key).expect("cool");
            let warm = warm_copy(cx, &image);
            assert!(cooled.finish_cool(token, warm, cx));
        }

        hot_only.set_budget(tight, cx);
        cooled.set_budget(tight, cx);

        assert_eq!(
            hot_only.display_entry_count(),
            8,
            "eight 1 KiB rasters fill an 8 KiB slice"
        );
        assert_eq!(
            cooled.display_entry_count(),
            12,
            "the same slice holds every entry once eight of them cost 24 bytes instead of 1024"
        );
        assert_eq!(cooled.warm_entry_count(), 8);
        assert_eq!(cooled.account().display_bytes, 4 * 1024);
    });
}
