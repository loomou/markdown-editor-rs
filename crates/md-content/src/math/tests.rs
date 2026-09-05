use super::raster::{layout_em, raster_dimensions};
use super::{
    MAX_BYTES, MAX_ENTRIES, MAX_METRIC_ENTRIES, MathCache, RasterOut, RasterSpec,
    WARM_EXTRA_ENTRIES, key_for, metric,
};
use crate::math::metrics::MathEm;
use crate::math::raster::raster;
use md_core::block::BlockKind;
use md_theme::DocumentTheme;

#[test]
fn layout_em_has_height_and_depth() {
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;
    let em = layout_em("x", false, color).expect("layout");
    assert!(em.width > 0.0);
    assert!(em.height > 0.0);
    assert!(em.depth >= 0.0);
    assert!(em.height + em.depth > 0.2);
}

#[test]
fn heading_em_png_taller_than_body() {
    let theme = DocumentTheme::formal();
    let color = theme.type_role(BlockKind::Paragraph).color;
    let body = RasterSpec {
        display: false,
        font_size: theme.type_role(BlockKind::Paragraph).size_px,
        dpr: 1.0,
        color,
    };
    let heading = RasterSpec {
        display: false,
        font_size: theme.type_role(BlockKind::Heading(1)).size_px,
        dpr: 1.0,
        color: theme.type_role(BlockKind::Heading(1)).color,
    };
    let b = raster("E=mc^2", &body).expect("body").image.expect("png");
    let h = raster("E=mc^2", &heading)
        .expect("heading")
        .image
        .expect("png");
    assert!(h.px_h > b.px_h);
    assert!(heading.font_size > body.font_size);
}

#[test]
fn estimate_is_not_max_box() {
    let e = MathEm::estimate("a");
    let w = e.css_width(16.0);
    let h = e.css_height(16.0);
    assert!(w < 80.0);
    assert!(h < 40.0);
}

#[test]
fn raster_png_matches_ceil_em() {
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;

    for display in [false, true] {
        for latex in ["x^2+y^2", "\\sum_{i=1}^{N} \\frac{1}{i}"] {
            let spec = RasterSpec {
                display,
                font_size: 16.0,
                dpr: 1.5,
                color,
            };
            let out = raster(latex, &spec).expect("layout");
            let img = out.image.expect("png");
            let w = (out.em.css_width(16.0) * 1.5).ceil() as u32;
            let h = (out.em.css_height(16.0) * 1.5).ceil() as u32;
            assert_eq!(
                img.px_w,
                w.max(1),
                "{latex} display={display} width mismatch"
            );
            assert_eq!(
                img.px_h,
                h.max(1),
                "{latex} display={display} height mismatch"
            );
        }
    }
}

#[test]
fn display_layout_uses_display_style_metrics() {
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;
    let latex = r"\sum_{i=1}^{N} \frac{1}{i}";
    let text = layout_em(latex, false, color).expect("text layout");
    let display = layout_em(latex, true, color).expect("display layout");
    assert_ne!(
        (
            text.width.to_bits(),
            text.height.to_bits(),
            text.depth.to_bits()
        ),
        (
            display.width.to_bits(),
            display.height.to_bits(),
            display.depth.to_bits()
        ),
        "display math must use its distinct ratex style"
    );
}

#[test]
fn raster_gives_up_on_unparsable_latex() {
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;
    let spec = RasterSpec {
        display: false,
        font_size: 16.0,
        dpr: 1.0,
        color,
    };

    assert!(raster("a^", &spec).is_err());
    assert!(raster("x_", &spec).is_err());
    assert!(raster("a^2", &spec).is_ok());
}

#[test]
fn raster_rejects_a_formula_that_exceeds_the_pixel_budget() {
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;
    let spec = RasterSpec {
        display: false,
        font_size: 16.0,
        dpr: 1.0,
        color,
    };
    let list = ratex_types::display_item::DisplayList {
        items: Vec::new(),
        width: 100_000.0,
        height: 1.0,
        depth: 1.0,
    };
    assert!(raster_dimensions(&list, &spec).is_none());
}

#[test]
fn record_failure_marks_metrics_none_and_bumps_gen_once() {
    let mut cache = MathCache::new();
    let key = key_for(
        "a^",
        false,
        16.0,
        DocumentTheme::formal()
            .type_role(BlockKind::Paragraph)
            .color,
        1.0,
    );

    assert!(cache.record_failure(&key));
    let gen_after_first = cache.metrics_gen();
    assert_eq!(gen_after_first, 1);
    let metrics = cache.metrics_snapshot();
    assert_eq!(metrics.len(), 1);
    assert!(matches!(metric(&metrics, "a^", false), Some(None)));

    assert!(!cache.record_failure(&key));
    assert_eq!(cache.metrics_gen(), gen_after_first);
    assert_eq!(cache.metrics_snapshot().len(), 1);
}

#[test]
fn successful_metric_duplicates_do_not_bump_generation() {
    let theme = DocumentTheme::formal();
    let color = theme.type_role(BlockKind::Paragraph).color;
    let mut cache = MathCache::new();
    let a = key_for("x", false, 16.0, color, 1.0);
    let b = key_for("x", false, 18.0, color, 1.0);
    assert!(cache.begin(a.clone()));
    let out = raster(
        "x",
        &RasterSpec {
            display: false,
            font_size: 16.0,
            dpr: 1.0,
            color,
        },
    )
    .expect("raster");
    assert!(cache.finish_inner(a, Ok(out), None));
    assert_eq!(cache.metrics_gen(), 1);
    assert!(cache.begin(b.clone()));
    let out = raster(
        "x",
        &RasterSpec {
            display: false,
            font_size: 18.0,
            dpr: 1.0,
            color,
        },
    )
    .expect("raster");
    assert!(cache.finish_inner(b, Ok(out), None));
    assert_eq!(cache.metrics_gen(), 1);
}

#[test]
fn failed_formula_slots_obey_the_entry_limit() {
    let mut cache = MathCache::new();
    let color = DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color;
    for i in 0..MAX_ENTRIES + 8 {
        let key = key_for(&format!("broken-{i}"), false, 16.0, color, 1.0);
        assert!(cache.begin(key.clone()));
        assert!(cache.finish_inner(
            key,
            Err(crate::Error::Math("broken".to_string().into())),
            None,
        ));
    }
    assert_eq!(cache.entry_count(), MAX_ENTRIES);

    assert_eq!(cache.metrics_snapshot().len(), MAX_ENTRIES + 8);
}

#[test]
fn box_size_lands_on_whole_device_pixels() {
    let em = MathEm {
        width: 1.359,
        height: 0.7,
        depth: 0.25,
    };
    for dpr in [1.0f32, 1.5, 2.0] {
        let w = em.box_width(16.0, dpr) * dpr;
        let h = em.box_height(16.0, dpr) * dpr;
        assert!(
            (w - w.round()).abs() < 1e-3,
            "dpr={dpr} box width {w} is not a whole pixel"
        );
        assert!(
            (h - h.round()).abs() < 1e-3,
            "dpr={dpr} box height {h} is not a whole pixel"
        );
        assert!(
            w >= em.css_width(16.0) * dpr - 1e-3,
            "dpr={dpr} box is narrower than the formula"
        );
    }
}

fn dummy_ready() -> crate::pixels::ReadyImage {
    crate::pixels::from_bgra(image::RgbaImage::new(4, 4), 1.0).expect("ready")
}

fn dummy_out(img: crate::pixels::ReadyImage) -> RasterOut {
    RasterOut {
        em: MathEm {
            width: 1.0,
            height: 1.0,
            depth: 0.0,
        },
        image: Some(img),
    }
}

fn math_color() -> md_theme::ThemeColor {
    DocumentTheme::formal()
        .type_role(BlockKind::Paragraph)
        .color
}

fn fill_math_ready(cache: &mut MathCache, n: usize) {
    let color = math_color();
    for i in 0..n {
        let key = key_for(&format!("f{i}"), false, 16.0, color, 1.0);
        assert!(cache.begin(key.clone()));
        assert!(cache.finish_inner(key, Ok(dummy_out(dummy_ready())), None));
    }
}

const OVER_LIMIT: usize = WARM_EXTRA_ENTRIES + 28;

#[test]
fn working_set_trims_beyond_the_warm_window() {
    let mut cache = MathCache::new();
    fill_math_ready(&mut cache, OVER_LIMIT);
    assert_eq!(cache.entry_count(), OVER_LIMIT);
    let keep = key_for(
        &format!("f{}", OVER_LIMIT - 1),
        false,
        16.0,
        math_color(),
        1.0,
    );
    cache.set_working_set_inner([keep.clone()], std::iter::empty(), None);
    assert!(cache.contains(&keep));
    assert_eq!(cache.entry_count(), 1 + WARM_EXTRA_ENTRIES);
    assert!(!cache.contains(&key_for("f0", false, 16.0, math_color(), 1.0)));
}

#[test]
fn working_set_does_not_evict_inflight_or_hot() {
    let mut cache = MathCache::new();
    fill_math_ready(&mut cache, OVER_LIMIT);
    let hot = key_for(
        &format!("f{}", OVER_LIMIT - 1),
        false,
        16.0,
        math_color(),
        1.0,
    );
    let inflight = key_for("inflight", false, 16.0, math_color(), 1.0);
    assert!(cache.begin(inflight.clone()));
    cache.set_working_set_inner([hot.clone()], std::iter::empty(), None);
    assert!(cache.contains(&hot));
    assert!(cache.contains(&inflight));
    assert_eq!(cache.entry_count(), 1 + WARM_EXTRA_ENTRIES);
}

#[test]
fn warm_entries_outlive_cold_ones() {
    let mut cache = MathCache::new();
    fill_math_ready(&mut cache, OVER_LIMIT);
    let color = math_color();
    let hot = key_for(&format!("f{}", OVER_LIMIT - 1), false, 16.0, color, 1.0);

    let warm: Vec<_> = (0..3)
        .map(|i| key_for(&format!("f{i}"), false, 16.0, color, 1.0))
        .collect();

    cache.set_working_set_inner([hot.clone()], warm.iter().cloned(), None);

    assert!(
        cache.entry_count() < OVER_LIMIT,
        "this test's premise is that eviction actually happened"
    );
    for key in &warm {
        assert!(
            cache.contains(key),
            "warm entries must not be evicted while cold entries remain"
        );
    }
    assert!(cache.contains(&hot));

    assert!(!cache.contains(&key_for("f3", false, 16.0, color, 1.0)));
}

#[test]
fn metrics_survive_bitmap_eviction() {
    let mut cache = MathCache::new();
    fill_math_ready(&mut cache, OVER_LIMIT);
    let before = cache.metrics_snapshot().len();
    let gen_before = cache.metrics_gen();

    let hot = key_for(
        &format!("f{}", OVER_LIMIT - 1),
        false,
        16.0,
        math_color(),
        1.0,
    );
    cache.set_working_set_inner([hot], std::iter::empty(), None);

    assert!(
        cache.entry_count() < OVER_LIMIT,
        "this test's premise is that bitmaps were actually evicted"
    );
    assert_eq!(
        cache.metrics_snapshot().len(),
        before,
        "bitmap eviction must not take the metrics with it"
    );
    assert_eq!(
        cache.metrics_gen(),
        gen_before,
        "bitmap eviction must not advance the metrics generation, or it would trigger a needless whole-document relayout"
    );
}

#[test]
fn metrics_obey_their_own_entry_limit() {
    let mut cache = MathCache::new();
    let color = math_color();
    for i in 0..MAX_METRIC_ENTRIES + 16 {
        let key = key_for(&format!("m{i}"), false, 16.0, color, 1.0);
        assert!(cache.begin(key.clone()));
        assert!(cache.finish_inner(key, Ok(dummy_out(dummy_ready())), None));
    }
    assert_eq!(cache.metrics_snapshot().len(), MAX_METRIC_ENTRIES);
}

#[test]
fn bytes_are_bounded_by_the_hard_budget_only() {
    let mut cache = MathCache::new();
    let color = math_color();
    let each = 4 * 1024 * 1024;
    let n = MAX_BYTES / each + 2;
    for i in 0..n {
        let key = key_for(&format!("big{i}"), false, 16.0, color, 1.0);
        assert!(cache.begin(key.clone()));
        let mut img = dummy_ready();
        img.bytes = each;
        assert!(cache.finish_inner(key, Ok(dummy_out(img)), None));
    }

    assert!(cache.byte_count() <= MAX_BYTES);
    cache.set_working_set_inner(
        std::iter::empty::<super::MathKey>(),
        std::iter::empty(),
        None,
    );
    assert!(cache.byte_count() <= MAX_BYTES);

    assert!(cache.entry_count() <= WARM_EXTRA_ENTRIES);
}
