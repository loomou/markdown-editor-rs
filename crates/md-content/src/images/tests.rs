use super::keys::{cache_key, display_key};
use super::raster::{decoded_len, raster_display};
use super::source::{
    decode_data_payload, detect_format, gif_frame_count, is_image_path, markdown_dest,
    parse_retry_after, resolve, validate_image_dimensions, validate_svg_dimensions,
    webp_frame_count,
};
use super::{
    DataEncoding, ImageCache, MAX_FAILED_ENTRIES, MAX_IN_FLIGHT, Resolved, SOURCE_RETRY_BASE,
    SOURCE_RETRY_MAX, SourceError, SourceKey, WARM_DISPLAY_EXTRA, source_retry_delay,
};
use crate::images::fit::{contain_fit, width_fit};
use gpui::{ImageFormat, RenderImage};
use std::path::PathBuf;
use std::sync::Arc;

use image::RgbImage;
use smallvec::smallvec;
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

    let native = native_from_png(&tiny_png());
    cache.finish_source_inner(job, Ok(native), None, usize::MAX);
    assert!(cache.source_image("u").is_some());
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
    let native = native_from_png(&tiny_png());
    cache.finish_source_inner(job, Ok(native), None, usize::MAX);
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

    let native = native_from_png(&tiny_png());
    assert!(
        cache.finish_source_inner(retry_job, Ok(native), None, usize::MAX),
        "a completion while the retry slot is alive must be booked"
    );
    assert!(cache.source_image("retry").is_some());
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
    let native = native_from_png(&png);
    let mut cache = ImageCache::new();
    let job = cache.begin_source("k".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native.clone()), None, usize::MAX));
    assert_eq!(cache.intrinsic("k"), Some((4, 3)));
    let key = display_key("k", 4.0, 3.0, 1.0);
    let djob = cache.begin_display(key.clone()).expect("begin display");
    let ready = raster_display(&native.0, &key).expect("raster");
    assert!(cache.finish_display_inner(djob, Ok(ready), None));
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
    let job = cache.begin_source("a".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native.clone()), None, source_bytes));
    let djob = cache.begin_display(a.clone()).expect("begin display");
    let display = raster_display(&native.0, &a).expect("display");
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
    assert!(cache.finish_source_inner(job, Ok(native.clone()), None, source_bytes));
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
    assert!(cache.finish_source_inner(job, Ok(native), None, source_bytes));
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
    let job = cache.begin_source("a".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native.clone()), None, budget));

    let b = display_key("b", 4.0, 3.0, 1.0);
    cache.set_working_set_inner(
        [a.clone(), b.clone()],
        std::iter::empty(),
        std::iter::empty(),
        None,
        budget,
    );
    let job = cache.begin_source("b".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native.clone()), None, budget));
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
    assert!(cache.finish_source_inner(job, Ok(native), None, budget));
    assert_eq!(cache.intrinsic("b"), Some((4, 3)));
    assert!(
        !cache.source_contains("a"),
        "booking a retry should evict the offscreen a"
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
    let job = cache.begin_source("k".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native), None, 48));
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
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &gif_with_canvas_and_frames(4000, 4000, 2))
            .is_ok()
    );
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &gif_with_canvas_and_frames(4, 4, 3)).is_ok()
    );
}

#[test]
fn gifs_with_more_frames_than_the_animation_cap_are_rejected() {
    let bomb = gif_with_canvas_and_frames(4, 4, 4097);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &bomb).is_err(),
        "4097 frames must be refused by the frame cap"
    );
    let capped = gif_with_canvas_and_frames(4, 4, 4096);
    assert!(
        validate_image_dimensions(ImageFormat::Gif, &capped).is_ok(),
        "4096 frames, within the cap, must pass"
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
    let native = native_from_png(&tiny_png());
    for i in 0..n {
        let dest = format!("d{i}");
        let key = display_key(&dest, 4.0, 3.0, 1.0);
        let job = cache.begin_source(dest).expect("begin");
        assert!(cache.finish_source_inner(job, Ok(native.clone()), None, usize::MAX));
        let djob = cache.begin_display(key.clone()).expect("begin display");
        let display = raster_display(&native.0, &key).expect("display");
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
    let native = native_from_png(&tiny_png());
    let job = cache.begin_source("inf".into()).expect("begin");
    assert!(cache.finish_source_inner(job, Ok(native), None, usize::MAX));
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
