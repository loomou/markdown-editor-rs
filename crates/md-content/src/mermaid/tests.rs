use super::raster::render_png;
use super::svg::{
    MAX_RASTER_PIXELS, first_viewbox, pad_svg_viewbox, raster_dpr, raster_scale, strip_init,
};
use super::{
    FAIL_LABEL_CAP, MAX_ENTRIES, MAX_FITTED_ENTRIES, MermaidCache, MermaidKey, RasterSpec,
    WARM_EXTRA_ENTRIES, fail_label, fit_rect,
};
use crate::mermaid::raster::raster;
use crate::mermaid::theme::{spec_from_theme, theme_fingerprint};
use crate::pixels::from_bgra;
use image::RgbaImage;

#[test]
fn strip_init_drops_directive() {
    let src = "%%{init: {'theme':'dark'}}%%\nflowchart TD\nA-->B\n";
    let out = strip_init(src);
    assert_eq!(out, "\nflowchart TD\nA-->B\n");
}

#[test]
fn strip_init_removes_each_closed_directive_and_preserves_unclosed_text() {
    let src = "%%{init: {one}}%%flowchart TD\n%%{init: {two}}%%A-->B\n";
    assert_eq!(
        strip_init(src),
        "flowchart TD\nA-->B\n",
        "all closed init directives must be removed"
    );
    let unclosed = "flowchart TD\n%%{init: {unfinished}\nA-->B\n";
    assert_eq!(strip_init(unclosed), unclosed);
}

#[test]
fn pad_svg_viewbox_expands_root_only() {
    let svg = r#"<svg viewBox="0 0 10 20"><marker viewBox="0 0 10 10"/></svg>"#;
    let out = pad_svg_viewbox(svg, 2.0);
    assert!(out.contains(r#"viewBox="-2 -2 14 24""#));
    assert!(out.contains(r#"viewBox="0 0 10 10""#));
    let (x, y, w, h) = first_viewbox(&out).expect("vb");
    assert_eq!((x, y, w, h), (-2.0, -2.0, 14.0, 24.0));
}

#[test]
fn first_viewbox_ignores_nested_marker_attributes() {
    let svg = r#"<svg><marker viewBox="0 0 10 10"/></svg>"#;
    assert_eq!(first_viewbox(svg), None);
    assert_eq!(pad_svg_viewbox(svg, 2.0), svg);
}

fn spec_fit(w: u32, h: u32) -> RasterSpec {
    spec_for(md_theme::DocumentTheme::formal(), w, h)
}

fn spec_for(theme: md_theme::DocumentTheme, w: u32, h: u32) -> RasterSpec {
    let key = MermaidKey {
        block: 0,
        generation: 1,
        revision: 1,
        theme_fp: theme_fingerprint(&theme),
        width_q: w,
        max_h: h,
        dpr_q: 4,
    };
    spec_from_theme(&theme, key)
}

#[test]
fn spec_splits_canvas_surface_cluster_and_line() {
    for theme in [
        md_theme::DocumentTheme::formal(),
        md_theme::DocumentTheme::one_dark(),
        md_theme::DocumentTheme::one_light(),
    ] {
        let spec = spec_from_theme(
            &theme,
            MermaidKey {
                block: 0,
                generation: 1,
                revision: 1,
                theme_fp: theme_fingerprint(&theme),
                width_q: 804,
                max_h: 420,
                dpr_q: 4,
            },
        );
        assert_ne!(
            spec.canvas, spec.surface,
            "node must not share the canvas color"
        );
        assert_ne!(
            spec.surface, spec.cluster,
            "subgraph must not share the node color"
        );
        assert_ne!(
            spec.cluster, spec.canvas,
            "subgraph must not share the canvas color"
        );
        assert_ne!(
            spec.line, spec.subtle,
            "arrow must not share the disabled-text color"
        );

        assert_eq!(
            spec.surface,
            theme.app.panel_bg.to_css_hex(),
            "node surface must equal the panel slot"
        );
        assert_eq!(
            spec.cluster,
            theme.app.active.to_css_hex(),
            "subgraph surface must equal the pressed/track slot"
        );
    }
}

#[test]
fn theme_fingerprint_ignores_fields_outside_mermaid_raster_inputs() {
    let theme = md_theme::DocumentTheme::formal();
    let mut changed = theme;
    changed.paint.list_marker = md_theme::ThemeColor::new(0.12, 0.8, 0.4, 1.0);
    changed.type_scale.quote = md_theme::TypeRole {
        family: md_theme::SYSTEM_SERIF,
        ..theme.type_scale.quote
    };
    assert_eq!(theme_fingerprint(&theme), theme_fingerprint(&changed));
}

#[test]
fn theme_fingerprint_tracks_the_mermaid_font_family() {
    let theme = md_theme::DocumentTheme::formal();
    let mut changed = theme;
    changed.type_scale.body = md_theme::TypeRole {
        family: md_theme::SYSTEM_SERIF,
        ..theme.type_scale.body
    };
    assert_ne!(
        theme_fingerprint(&theme),
        theme_fingerprint(&changed),
        "body font changes flow into diagram text, so the fingerprint must change"
    );
}

#[test]
fn mermaid_font_family_maps_the_virtual_name_and_adds_a_fallback() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    assert_eq!(spec.font_family, "system-ui, sans-serif");
    let mut serif = md_theme::DocumentTheme::one_dark();
    serif.type_scale.body.family = md_theme::SYSTEM_SERIF;
    assert_eq!(spec_for(serif, 804, 420).font_family, "Georgia, sans-serif");
}

#[test]
fn theme_fingerprint_tracks_the_series_palette() {
    let theme = md_theme::DocumentTheme::one_dark();
    let mut changed = theme;
    changed.syntax.symbol = md_theme::ThemeColor::new(0.99, 0.8, 0.5, 1.0);
    assert_ne!(
        theme_fingerprint(&theme),
        theme_fingerprint(&changed),
        "the fingerprint must track the series palette, or stale PNGs never invalidate"
    );
}

#[test]
fn series_palette_covers_eight_hues_from_the_syntax_tokens() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    assert_eq!(
        spec.series,
        [
            "#e06c75", "#dfc184", "#e5c07b", "#a1c181", "#56b6c2", "#74ade8", "#b477cf", "#74ade8",
        ],
        "red, orange, yellow, green, cyan, blue, violet plus the accent, in the order pie/git take colors"
    );
}

#[test]
fn series_palette_reaches_pie_slices() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    let svg = super::raster::render_svg(
        "pie title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15\n",
        &spec,
    )
    .expect("svg");
    for hex in ["#e06c75", "#dfc184", "#e5c07b"] {
        assert!(
            svg.contains(hex),
            "the pie SVG must contain the series color {hex}"
        );
    }
}

#[test]
fn raster_svg_keeps_css_size_and_hits_the_density() {
    let theme = md_theme::DocumentTheme::one_dark();
    let spec = spec_for(theme, 804, 420);
    let src = "flowchart TD\n  A0-->B0\n  B0-->C0\n";
    let (doc, sealed) = super::raster::raster_with_svg(src, &spec).expect("raster");
    let css = doc.css_size();
    let zoomed = super::raster::raster_svg(&sealed, css, 3.0).expect("zoom raster");
    let (zw, zh) = zoomed.css_size();
    assert!(
        (zw - css.0).abs() < 1.0 && (zh - css.1).abs() < 1.0,
        "css size jumped: {css:?} -> ({zw}, {zh})"
    );
    assert!(
        (zoomed.px_w as f32 - css.0 * 3.0).abs() < 4.0,
        "px_w={} expected ≈{}",
        zoomed.px_w,
        css.0 * 3.0
    );
}

#[test]
fn raster_scale_keeps_small_diagram_layout_native() {
    let spec = spec_fit(804, 420);
    let svg = r#"<svg viewBox="0 0 94 278"></svg>"#;
    let scale = raster_scale(svg, &spec);
    let dpr = raster_dpr(svg, &spec);
    assert!(
        (scale / dpr - 1.0).abs() < 0.02,
        "small diagrams must not be shrunk by contain: scale={scale} dpr={dpr}"
    );
    assert!(
        dpr > spec.dpr + 0.5,
        "small diagrams need denser pixels for the zoom overlay: dpr={dpr}"
    );
    assert!(
        dpr <= spec.dpr * 2.0,
        "document diagrams must pre-bake at most 2×: dpr={dpr}"
    );
}

#[test]
fn raster_scale_obeys_the_pixel_budget() {
    let mut spec = spec_fit(10_000, 10_000);
    spec.dpr = 4.0;
    let svg = r#"<svg viewBox="0 0 10000 10000"></svg>"#;
    let scale = raster_scale(svg, &spec);
    let dpr = raster_dpr(svg, &spec);
    let pixels = 10_000.0_f64 * 10_000.0 * f64::from(scale).powi(2);
    assert!(
        pixels <= MAX_RASTER_PIXELS * 1.0001,
        "pixels={pixels} limit={MAX_RASTER_PIXELS}"
    );
    assert!((scale / dpr - 1.0).abs() < 0.001, "scale={scale} dpr={dpr}");
}

#[test]
fn raster_scale_downsizes_when_wider_than_slot() {
    let spec = spec_fit(804, 420);
    let svg = r#"<svg viewBox="0 0 1600 900"></svg>"#;
    let scale = raster_scale(svg, &spec);
    let dpr = raster_dpr(svg, &spec);
    let contain = (804.0_f32 / 1600.0).min(420.0 / 900.0);
    assert!(
        (scale / dpr - contain).abs() < 0.02,
        "large-diagram contain must match the slot: scale={scale} dpr={dpr} contain={contain}"
    );
}

#[test]
fn raster_scale_budgets_a_root_without_viewbox() {
    let mut spec = spec_fit(10_000, 10_000);
    spec.dpr = 4.0;
    let svg = r#"<svg width="10000" height="10000"></svg>"#;
    let scale = raster_scale(svg, &spec);
    let pixels = 10_000.0_f64 * 10_000.0 * f64::from(scale).powi(2);
    assert!(
        pixels <= MAX_RASTER_PIXELS * 1.0001,
        "no-viewBox pixels={pixels} limit={MAX_RASTER_PIXELS}"
    );
    let unknown = raster_scale("<svg></svg>", &spec);
    assert_eq!(
        unknown, 0.25,
        "unknown intrinsic size uses the minimum multiplier"
    );
}

#[test]
fn paint_rect_contains_a_stale_large_bitmap() {
    let (x, y, width, height) = fit_rect(400.0, 270.0, 1184.0, 800.0).expect("fit");
    assert!((width - 399.6).abs() < 0.1, "width={width}");
    assert!((height - 270.0).abs() < 0.1, "height={height}");
    assert!((x - 0.2).abs() < 0.1, "x={x}");
    assert!(y.abs() < 0.1, "y={y}");
    assert!((width / height - 1184.0 / 800.0).abs() < 0.001);
}

#[test]
fn mixed_flowchart_css_size_stays_near_native() {
    let spec = spec_fit(804, 420);
    let src = "flowchart TD\n  A0-->B0\n  B0-->C0\n";
    let ready = raster(src, &spec).expect("raster");
    let (cw, ch) = ready.css_size();
    assert!(ch > 240.0 && ch < 320.0, "css_h={ch}");
    assert!(cw > 80.0 && cw < 160.0, "css_w={cw}");
    assert!(
        ready.px_h as f32 > ch * 1.5 && ready.px_h as f32 <= ch * 2.1,
        "zooming must use denser pixels than the css raster: px_h={} css_h={ch}",
        ready.px_h
    );
}

#[test]
fn raster_leaves_the_diagram_background_transparent() {
    let spec = spec_fit(804, 420);
    let png = render_png("flowchart TD\n  A0-->B0\n", &spec).expect("png");
    let img = image::load_from_memory(&png).expect("decode").into_rgba8();
    let (w, h) = img.dimensions();
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        assert_eq!(
            img.get_pixel(x, y).0[3],
            0,
            "corner pixel ({x},{y}) must be transparent"
        );
    }
}

#[test]
fn failed_diagram_slots_obey_the_entry_limit() {
    let mut cache = MermaidCache::new();
    for block in 0..(MAX_ENTRIES as u32 + 8) {
        let key = MermaidKey {
            block,
            generation: 1,
            revision: 1,
            theme_fp: 1,
            width_q: 800,
            max_h: 600,
            dpr_q: 4,
        };
        assert!(cache.begin(key));
        cache.finish_inner(
            key,
            Err(crate::Error::Mermaid("broken".to_string().into())),
            None,
        );
    }
    assert_eq!(cache.entry_count(), MAX_ENTRIES);
}

fn ready_image(width: u32, height: u32, dpr: f32) -> crate::pixels::ReadyImage {
    from_bgra(RgbaImage::new(width, height), dpr).expect("ready image")
}

#[test]
fn image_falls_back_to_the_previous_ready_diagram_and_updates_fitted_size() {
    let mut cache = MermaidCache::new();
    let first = MermaidKey {
        block: 7,
        generation: 1,
        revision: 1,
        theme_fp: 1,
        width_q: 800,
        max_h: 600,
        dpr_q: 4,
    };
    assert!(cache.begin(first));
    let first_image = ready_image(80, 40, 1.0);
    cache.finish_inner(first, Ok(first_image), None);
    assert_eq!(
        cache.image(&first).map(|img| img.css_size()),
        Some((80.0, 40.0))
    );

    let second = MermaidKey {
        revision: 2,
        ..first
    };
    assert!(cache.begin(second));
    assert_eq!(
        cache.image(&second).map(|img| img.css_size()),
        Some((80.0, 40.0)),
        "an in-flight key should reuse the block's previous ready image"
    );
    let fitted = cache.fitted_snapshot();
    assert_eq!(fitted.get(&(7, 1)), Some(&(80.0, 40.0)));
    assert!(
        std::rc::Rc::ptr_eq(&fitted, &cache.fitted_snapshot()),
        "fitted snapshots must share the persistent projection"
    );

    cache.finish_inner(second, Ok(ready_image(120, 60, 2.0)), None);
    assert_eq!(
        cache.fitted_snapshot().get(&(7, 1)),
        Some(&(60.0, 30.0)),
        "the current projection must track the newest ready raster's CSS size"
    );
}

#[test]
fn evicting_an_old_key_does_not_remove_the_current_key_for_its_block() {
    let mut cache = MermaidCache::new();
    let old = MermaidKey {
        block: 11,
        generation: 1,
        revision: 1,
        theme_fp: 1,
        width_q: 800,
        max_h: 600,
        dpr_q: 4,
    };
    assert!(cache.begin(old));
    cache.finish_inner(old, Ok(ready_image(20, 20, 1.0)), None);
    let current = MermaidKey { revision: 2, ..old };
    assert!(cache.begin(current));
    cache.finish_inner(current, Ok(ready_image(30, 30, 1.0)), None);

    for block in 100..(100 + MAX_ENTRIES as u32 - 1) {
        let key = MermaidKey { block, ..old };
        assert!(cache.begin(key));
        cache.finish_inner(key, Ok(ready_image(4, 4, 1.0)), None);
    }

    assert!(
        !cache.contains(&old),
        "the stale key should be evicted first"
    );
    assert!(cache.contains(&current));
    assert_eq!(cache.fitted_snapshot().get(&(11, 1)), Some(&(30.0, 30.0)));
}

#[test]
fn paint_rect_snaps_position_with_window_dpr_not_raster_dpr() {
    let ready = ready_image(200, 400, 4.0);
    let raster_dpr_position = super::paint_rect(10.25, 0.0, 100.0, 100.0, &ready, 4.0)
        .expect("fit")
        .0;
    let window_dpr_position = super::paint_rect(10.25, 0.0, 100.0, 100.0, &ready, 1.0)
        .expect("fit")
        .0;
    assert_eq!(raster_dpr_position, 35.25);
    assert_eq!(window_dpr_position, 35.0);
}

#[test]
fn fail_label_caps_source_bearing_errors_on_char_boundaries() {
    let message = format!("cannot recognize the diagram type: {}", "é".repeat(10_000));
    let label = fail_label(&crate::Error::Mermaid(message.clone().into()));

    assert!(label.len() <= FAIL_LABEL_CAP, "{}", label.len());
    assert!(label.is_char_boundary(label.len()));
    assert!(
        message.starts_with(&label),
        "the label must be a prefix of the original message, never rewritten"
    );
}

#[test]
fn fail_label_passes_short_messages_through_verbatim() {
    let err = crate::Error::Mermaid(md_i18n::Key::DiagramEmpty.into());
    assert_eq!(fail_label(&err), err.to_string());
}

fn mermaid_key(block: u32) -> MermaidKey {
    MermaidKey {
        block,
        generation: 1,
        revision: 1,
        theme_fp: 1,
        width_q: 800,
        max_h: 600,
        dpr_q: 4,
    }
}

fn fill_mermaid_ready(cache: &mut MermaidCache, n: u32) {
    for block in 0..n {
        let key = mermaid_key(block);
        assert!(cache.begin(key));
        cache.finish_inner(key, Ok(ready_image(4, 4, 1.0)), None);
    }
}

#[test]
fn working_set_trims_beyond_the_warm_window() {
    let mut cache = MermaidCache::new();
    fill_mermaid_ready(&mut cache, 24);
    assert_eq!(cache.entry_count(), 24);
    let keep = mermaid_key(23);
    cache.set_working_set_inner([keep], std::iter::empty(), None);
    assert!(cache.contains(&keep));
    assert_eq!(cache.entry_count(), 1 + WARM_EXTRA_ENTRIES);
    assert!(!cache.contains(&mermaid_key(0)));
}

#[test]
fn working_set_does_not_evict_inflight_or_hot() {
    let mut cache = MermaidCache::new();
    fill_mermaid_ready(&mut cache, 20);
    let hot = mermaid_key(19);
    let inflight = mermaid_key(99);
    assert!(cache.begin(inflight));
    cache.set_working_set_inner([hot], std::iter::empty(), None);
    assert!(cache.contains(&hot));
    assert!(cache.contains(&inflight));
    assert_eq!(cache.entry_count(), 1 + WARM_EXTRA_ENTRIES);
}

#[test]
fn warm_entries_outlive_cold_ones() {
    let mut cache = MermaidCache::new();
    fill_mermaid_ready(&mut cache, 24);
    let hot = mermaid_key(23);

    let warm = [mermaid_key(0), mermaid_key(1), mermaid_key(2)];

    cache.set_working_set_inner([hot], warm, None);

    assert!(
        cache.entry_count() < 24,
        "this test's premise is that eviction actually happened"
    );
    for key in &warm {
        assert!(
            cache.contains(key),
            "warm entries must not be evicted while cold entries remain"
        );
    }
    assert!(cache.contains(&hot));

    assert!(!cache.contains(&mermaid_key(3)));
}

#[test]
fn fitted_sizes_survive_bitmap_eviction() {
    let mut cache = MermaidCache::new();
    fill_mermaid_ready(&mut cache, 24);
    let before = cache.fitted_snapshot().len();
    assert_eq!(before, 24);

    cache.set_working_set_inner([mermaid_key(23)], std::iter::empty(), None);

    assert!(
        cache.entry_count() < 24,
        "this test's premise is that bitmaps were actually evicted"
    );
    assert!(
        !cache.contains(&mermaid_key(0)),
        "the first diagram's bitmap should already be gone"
    );
    assert_eq!(
        cache.fitted_snapshot().len(),
        before,
        "bitmap eviction must not take the size projection with it"
    );
    assert_eq!(
        cache.fitted_snapshot().get(&(0, 1)),
        Some(&(4.0, 4.0)),
        "the measured size of this block must survive the bitmap's eviction"
    );
}

#[test]
fn fitted_sizes_obey_their_own_entry_limit() {
    let mut cache = MermaidCache::new();
    for generation in 1..(MAX_FITTED_ENTRIES as u32 + 16) {
        let key = MermaidKey {
            generation,
            ..mermaid_key(5)
        };
        assert!(cache.begin(key));
        cache.finish_inner(key, Ok(ready_image(4, 4, 1.0)), None);
    }
    assert_eq!(cache.fitted_snapshot().len(), MAX_FITTED_ENTRIES);
}

#[test]
fn byte_pressure_still_spares_the_warm_window() {
    let mut cache = MermaidCache::new();
    let big = 8 * 1024 * 1024;
    for block in 0..6 {
        let key = mermaid_key(block);
        assert!(cache.begin(key));
        let mut img = ready_image(4, 4, 1.0);
        img.bytes = big;
        cache.finish_inner(key, Ok(img), None);
    }
    let warm = [mermaid_key(0), mermaid_key(1)];
    cache.set_working_set_inner([mermaid_key(5)], warm, None);

    assert_eq!(cache.byte_count(), 6 * big);
    assert_eq!(cache.entry_count(), 6);
    for key in &warm {
        assert!(
            cache.contains(key),
            "the warm window must stay untouched while bytes are within budget"
        );
    }
}

#[test]
fn render_svg_pads_the_root_viewbox() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    let svg = super::raster::render_svg("flowchart TD\n  A0-->B0\n", &spec).expect("svg");
    let (x, y, w, h) = first_viewbox(&svg).expect("root viewBox");
    assert_eq!(
        (x, y),
        (-super::VIEWBOX_PAD, -super::VIEWBOX_PAD),
        "flowchart natively starts at (0,0), so the origin proves the pad applied: {x} {y}"
    );
    assert!(w > 0.0 && h > 0.0);
}

#[test]
fn long_labels_render_as_a_single_line_fallback() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    let src = "flowchart TD\n  A[\"Pass 2: search transcript with annotation blocks excised, map offsets back to buffer space\"] --> B[\"Error describing where matches were found\"]\n";
    let svg = super::raster::render_svg(src, &spec).expect("svg");
    assert!(
        svg.contains("annotation blocks"),
        "label text must be fully present in the SVG"
    );
    assert!(
        !svg.contains("foreignObject"),
        "resvg-safe output must not contain foreignObject"
    );
    assert_eq!(
        svg.matches("tspan").count(),
        0,
        "the resvg-safe fallback must be a single line"
    );
}

#[test]
fn series_palette_reaches_git_branches() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    let src = "gitGraph\n  commit id: \"one\"\n  branch feature\n  commit\n  checkout main\n  commit\n  merge feature\n";
    let svg = super::raster::render_svg(src, &spec).expect("svg");
    for hex in ["#e06c75", "#dfc184"] {
        assert!(
            svg.contains(hex),
            "the gitGraph SVG must contain the series color {hex}"
        );
    }
}

#[test]
fn core_diagram_types_render_on_the_pinned_features() {
    let spec = spec_for(md_theme::DocumentTheme::one_dark(), 804, 420);
    let zoo = [
        "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running: start\n  Running --> [*]: stop\n",
        "classDiagram\n  Animal <|-- Dog\n  Animal: +int age\n  Dog: +bark()\n",
        "xychart-beta\n  title Test\n  x-axis [a, b, c]\n  y-axis \"Score\" 0 --> 10\n  bar [1, 5, 9]\n",
        "mindmap\n  root((Topic))\n    A\n      a1\n    B\n",
        "gantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  section S1\n  Task a :a1, 2026-01-01, 3d\n",
    ];
    for src in zoo {
        let svg = super::raster::render_svg(src, &spec)
            .unwrap_or_else(|e| panic!("diagram zoo member failed: {e}\n{src}"));
        assert!(!svg.is_empty());
    }
}

#[test]
fn prefetch_leaves_room_for_the_visible_diagram() {
    let mut cache = MermaidCache::new();
    let hot = mermaid_key(0);
    let warm = [mermaid_key(1), mermaid_key(2)];
    cache.set_working_set_inner([hot], warm, None);

    assert!(cache.begin(warm[0]), "the first prefetch must get a slot");
    assert!(
        !cache.begin(warm[1]),
        "once prefetches fill their quota, no new ones may start"
    );
    assert!(
        cache.begin(hot),
        "the on-screen diagram must still begin without waiting for the prefetches"
    );

    assert!(!cache.begin(mermaid_key(3)));
}
