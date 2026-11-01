use super::{MediaCache, MediaLayer, MediaStats, MediaWorkingSet};
use crate::images::DisplayKey;
use crate::math::{MathEm, RasterOut, key_for as math_key_for};
use crate::pixels::{ReadyImage, dpr_q, from_bgra};
use gpui::RenderImage;
use image::RgbaImage;
use md_theme::ThemeColor;
use smallvec::smallvec;
use std::sync::Arc;

fn ready(w: u32, h: u32) -> ReadyImage {
    from_bgra(RgbaImage::new(w, h), 1.0).expect("ready image")
}

fn native(w: u32, h: u32) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(smallvec![image::Frame::new(
        RgbaImage::new(w, h)
    )]))
}

fn store_math(cache: &mut MediaCache, latex: &str, w: u32, h: u32) {
    let key = math_key_for(latex, false, 16.0, ThemeColor::new(0.0, 0.0, 0.0, 1.0), 1.0);
    assert!(cache.math_mut().begin(key.clone()));
    let out = RasterOut {
        em: MathEm {
            width: 1.0,
            height: 0.7,
            depth: 0.25,
        },
        image: Some(ready(w, h)),
        prepared: None,
    };
    assert!(cache.math_mut().finish_inner(key, Ok(out), None));
}

fn store_source(cache: &mut MediaCache, dest: &str, w: u32, h: u32) {
    let token = cache
        .images_mut()
        .begin_source(dest.to_string())
        .expect("source token");
    assert!(cache.images_mut().finish_source_inner(
        token,
        Ok((native(w, h), w, h)),
        None,
        usize::MAX
    ));
}

fn store_display(cache: &mut MediaCache, dest: &str, w: u32, h: u32) {
    let key = DisplayKey {
        dest: dest.to_string(),
        width_q: 320,
        height_q: 240,
        dpr_q: dpr_q(1.0),
    };
    let token = cache
        .images_mut()
        .begin_display(key)
        .expect("display token");
    assert!(
        cache
            .images_mut()
            .finish_display_inner(token, Ok(ready(w, h)), None)
    );
}

#[test]
fn an_empty_cache_holds_nothing() {
    let cache = MediaCache::new();
    assert_eq!(cache.stats(), MediaStats::default());
    assert_eq!(cache.stats().total(), 0);
    assert_eq!(cache.total_bytes(), super::DEFAULT_TOTAL_BYTES);
}

#[test]
fn stats_add_up_across_every_kind() {
    let mut cache = MediaCache::new();
    store_math(&mut cache, "x^2", 10, 10);
    store_source(&mut cache, "a.png", 20, 20);
    store_display(&mut cache, "a.png", 4, 4);

    let stats = cache.stats();
    assert_eq!(stats.math_bytes, 400);
    assert_eq!(stats.source_bytes, 1600);
    assert_eq!(stats.display_bytes, 64);
    assert_eq!(stats.total(), 2064);
    assert_eq!(stats.layer(MediaLayer::Math), 400);
    assert_eq!(stats.layer(MediaLayer::Source), 1600);
}

#[test]
fn a_cache_under_its_total_budget_is_left_alone() {
    let mut cache = MediaCache::with_total_bytes(1 << 20);
    store_source(&mut cache, "a.png", 20, 20);
    store_math(&mut cache, "x", 10, 10);

    assert_eq!(cache.enforce_inner(None), 0);
    assert_eq!(cache.stats().source_bytes, 1600);
    assert_eq!(cache.stats().math_bytes, 400);
}

#[test]
fn the_total_budget_is_taken_back_from_whichever_layer_holds_it() {
    let mut cache = MediaCache::with_total_bytes(1_000);
    store_source(&mut cache, "big.png", 100, 100);
    assert_eq!(cache.stats().source_bytes, 40_000);

    assert!(cache.enforce_inner(None) > 0);
    assert!(
        cache.stats().total() <= 1_000,
        "total {} exceeds the budget",
        cache.stats().total()
    );
    assert_eq!(cache.stats().source_bytes, 0);
}

#[test]
fn pinned_media_is_not_evicted_to_meet_the_budget() {
    let mut cache = MediaCache::with_total_bytes(5_000);
    let pinned = DisplayKey {
        dest: "a.png".to_string(),
        width_q: 320,
        height_q: 240,
        dpr_q: dpr_q(1.0),
    };
    store_display(&mut cache, "a.png", 20, 20);
    store_source(&mut cache, "a.png", 20, 20);
    store_source(&mut cache, "b.png", 100, 100);
    assert_eq!(cache.stats().total(), 43_200);

    cache.images_mut().set_working_set_inner(
        std::iter::once(pinned),
        std::iter::empty::<String>(),
        std::iter::empty::<String>(),
        None,
        usize::MAX,
    );
    cache.enforce_inner(None);

    let stats = cache.stats();
    assert_eq!(stats.display_bytes, 1600, "the visible raster stays");
    assert_eq!(
        stats.source_bytes, 1600,
        "the source the visible raster derives from stays, the other one pays"
    );
    assert!(stats.total() <= 5_000);
}

#[test]
fn a_budget_below_the_pinned_floor_yields_instead_of_evicting_visible_media() {
    let mut cache = MediaCache::with_total_bytes(1);
    let pinned = DisplayKey {
        dest: "a.png".to_string(),
        width_q: 320,
        height_q: 240,
        dpr_q: dpr_q(1.0),
    };
    store_display(&mut cache, "a.png", 20, 20);
    store_source(&mut cache, "a.png", 20, 20);
    cache.images_mut().set_working_set_inner(
        std::iter::once(pinned),
        std::iter::empty::<String>(),
        std::iter::empty::<String>(),
        None,
        usize::MAX,
    );

    assert_eq!(cache.enforce_inner(None), 0);
    assert_eq!(cache.stats().total(), 3_200);
}

#[gpui::test]
fn clear_drops_every_layer(cx: &mut gpui::TestAppContext) {
    let mut cache = MediaCache::new();
    store_math(&mut cache, "x", 10, 10);
    store_source(&mut cache, "a.png", 10, 10);
    store_display(&mut cache, "a.png", 10, 10);
    assert!(cache.stats().total() > 0);

    cx.update(|cx| cache.clear(cx));
    assert_eq!(cache.stats(), MediaStats::default());
}

#[gpui::test]
fn one_working_set_covers_every_kind(cx: &mut gpui::TestAppContext) {
    let mut cache = MediaCache::with_total_bytes(1);
    store_math(&mut cache, "x", 20, 20);
    store_source(&mut cache, "a.png", 20, 20);
    let math_key = math_key_for("x", false, 16.0, ThemeColor::new(0.0, 0.0, 0.0, 1.0), 1.0);

    cx.update(|cx| {
        cache.submit_working_set(
            MediaWorkingSet {
                mermaid_hot: Vec::new(),
                mermaid_warm: Vec::new(),
                math_hot: vec![math_key],
                image_hot: Vec::new(),
                image_hot_sources: vec!["a.png".to_string()],
                image_warm_sources: Vec::new(),
            },
            cx,
        )
    });

    let stats = cache.stats();
    assert_eq!(stats.math_bytes, 1600, "the visible formula stays");
    assert_eq!(stats.source_bytes, 1600, "the visible source stays");
}

#[gpui::test]
fn a_released_image_outlives_the_update_that_released_it(cx: &mut gpui::TestAppContext) {
    let image = native(8, 8);
    let weak = Arc::downgrade(&image);
    cx.update(|cx| {
        super::drop_image_deferred(cx, image);
        assert!(
            weak.upgrade().is_some(),
            "dropping the atlas entry inside an update would miss the window that is taken out of the app"
        );
    });
    assert!(
        weak.upgrade().is_none(),
        "the release must run once the update is over"
    );
}

#[gpui::test]
fn an_evicted_raster_is_released_after_the_update_ends(cx: &mut gpui::TestAppContext) {
    let mut cache = MediaCache::with_total_bytes(1);
    store_math(&mut cache, "x", 20, 20);
    let key = math_key_for("x", false, 16.0, ThemeColor::new(0.0, 0.0, 0.0, 1.0), 1.0);
    let held = cache
        .math()
        .ready_snapshot()
        .get(&key)
        .cloned()
        .expect("ready raster");
    let weak = Arc::downgrade(&held.image);
    drop(held);

    cx.update(|cx| {
        cache.enforce(cx);
        assert_eq!(cache.stats().math_bytes, 0, "the ledger drops the entry");
        assert!(
            weak.upgrade().is_some(),
            "but the allocation is still held until the update ends"
        );
    });
    assert!(weak.upgrade().is_none());
}

#[test]
fn a_quiet_cache_owes_exactly_what_it_holds() {
    let mut cache = MediaCache::new();
    store_math(&mut cache, "x", 10, 10);
    store_source(&mut cache, "a.png", 20, 20);
    store_display(&mut cache, "a.png", 4, 4);

    assert_eq!(cache.resident_bytes(), cache.stats().total());
    assert_eq!(cache.resident_bytes(), 2_064);
}

#[test]
fn a_raster_held_outside_the_cache_is_not_written_off_early() {
    let mut cache = MediaCache::with_total_bytes(1);
    store_math(&mut cache, "x", 20, 20);
    let key = math_key_for("x", false, 16.0, ThemeColor::new(0.0, 0.0, 0.0, 1.0), 1.0);
    let held = cache
        .math()
        .ready_snapshot()
        .get(&key)
        .cloned()
        .expect("ready raster");
    assert_eq!(cache.resident_bytes(), 1_600);

    cache.enforce_inner(None);
    assert_eq!(cache.stats().math_bytes, 0, "the cache let the entry go");
    assert_eq!(
        cache.resident_bytes(),
        1_600,
        "the pixels are still resident, so the account must still show them"
    );

    drop(held);
    assert_eq!(
        cache.resident_bytes(),
        0,
        "and the account closes once the last reference is gone"
    );
}

#[test]
fn a_preview_outside_the_cache_is_reported_as_an_exception() {
    let mut cache = MediaCache::with_total_bytes(1_000);
    store_math(&mut cache, "x", 10, 10);
    let preview = ready(20, 20);
    cache.track(&preview.image, preview.bytes);

    let report = cache.report();
    assert_eq!(report.budget, 1_000);
    assert_eq!(report.cache_bytes, 400);
    assert_eq!(report.resident_bytes, 2_000);
    assert_eq!(report.over_budget, 1_000);
    assert_eq!(
        report.untracked_bytes(),
        1_600,
        "a preview is resident without being something the cache can evict"
    );

    drop(preview);
    let report = cache.report();
    assert_eq!(report.resident_bytes, 400);
    assert_eq!(report.over_budget, 0);
}

#[gpui::test]
fn closing_settles_every_layer_once_the_update_ends(cx: &mut gpui::TestAppContext) {
    let mut cache = MediaCache::new();
    store_math(&mut cache, "x", 10, 10);
    store_source(&mut cache, "a.png", 10, 10);
    store_display(&mut cache, "a.png", 10, 10);
    assert_eq!(cache.resident_bytes(), 1_200);

    cx.update(|cx| cache.clear(cx));
    assert_eq!(
        cache.resident_bytes(),
        0,
        "every layer is released and the account closes"
    );
}

#[test]
fn an_account_names_the_layer_every_byte_came_from() {
    let mut cache = MediaCache::with_total_bytes(1 << 20);
    store_source(&mut cache, "a.png", 10, 10);
    store_display(&mut cache, "a.png", 10, 10);
    store_math(&mut cache, "x", 10, 10);

    let account = cache.account(300);
    assert_eq!(account.source_bytes, 400);
    assert_eq!(account.source_entries, 1);
    assert_eq!(account.display_bytes, 400);
    assert_eq!(account.display_entries, 1);
    assert_eq!(account.math_bytes, 400);
    assert_eq!(account.math_entries, 1);
    assert_eq!(account.mermaid_bytes, 0);
    assert_eq!(account.mermaid_entries, 0);
    assert_eq!(account.zoom_bytes, 300);
    assert_eq!(account.cache_bytes, 1_200);
    assert_eq!(account.resident_bytes, 1_200);
    assert_eq!(
        account.media_bytes(),
        1_500,
        "a preview counts toward what the document holds"
    );
    assert_eq!(account.cache_bytes, 1_200, "but the cache cannot evict it");
    assert_eq!(account.entries(), 3);
    assert_eq!(account.untracked_bytes(), 0);
}
