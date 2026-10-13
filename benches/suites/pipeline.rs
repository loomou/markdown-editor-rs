use gpui::{AppContext as _, BenchAppContext};
use md_bench::gpui_support::{
    FrameCase, ProbeView, VIEWPORT_H, cases, count_hot_path, empty_media, text_nodes,
};
use md_bench::harness::first_leaf;
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_core::doc::Cursor;
use md_layout::island::FallbackSolver;
use md_render::frame::{FrameContext, FrameRequest, compose};
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_render::snap::SnapOperator;

#[gpui::bench(inputs = cases(), input_name = "case", group = "content/shaper-new", sample_size = 20)]
fn content_shaper_new(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let view_id =
        window.update(|window, app| window.replace_root(app, |_, _| ProbeView).entity_id());
    let theme = &case.theme;
    cx.bench_iter(move |cx| {
        let built = cx.with_window(view_id, |window, app| {
            GpuiShaper::new(window, app, theme, 1.0, ShapeCache::new(), empty_media())
        });
        assert!(built.is_some(), "the probe window must still be open");
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "content/artifact", sample_size = 20)]
fn content_artifact(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let (tree, nodes) = text_nodes(&case.doc, &case.theme);
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(window, app, &case.theme, 1.0, cache.clone(), empty_media())
    });
    let env = case.env;
    let node_count = nodes.len();
    cx.bench_iter(move |_cx| {
        for id in &nodes {
            let node = tree.get(*id);
            shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
        }
    });
    eprintln!(
        "[artifact {}] text-nodes={} cache-entries={} cache-bytes={} stats={:?}",
        case.label,
        node_count,
        cache.entry_count(),
        cache.byte_count(),
        cache.stats()
    );
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "content/artifact-cold", sample_size = 20)]
fn content_artifact_cold(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let (tree, nodes) = text_nodes(&case.doc, &case.theme);
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(window, app, &case.theme, 1.0, cache.clone(), empty_media())
    });
    let env = case.env;
    let node_count = nodes.len();
    let frame_cache = cache.clone();
    let mut epoch = 0u64;
    cx.bench_iter(move |_cx| {
        epoch += 1;
        frame_cache.begin_frame(epoch);
        for id in &nodes {
            let node = tree.get(*id);
            shaper.artifact(
                tree.text_of(node),
                tree.runs_of(node),
                tree.content_width(*id, env.viewport_width),
                node.kind(),
                node.shape_ident(),
            );
        }
        frame_cache.end_frame();
    });
    eprintln!(
        "[artifact-cold {}] text-nodes={} stats={:?}",
        case.label,
        node_count,
        cache.stats()
    );
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/frame", sample_size = 10)]
fn render_frame(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let (shaper, snap) = window.update(|window, app| {
        (
            GpuiShaper::new(
                window,
                app,
                &case.theme,
                1.0,
                ShapeCache::new(),
                empty_media(),
            ),
            SnapOperator::new(1.0),
        )
    });
    let block = first_leaf(&case.doc);
    let env = case.env;
    let doc = &case.doc;
    let theme = &case.theme;
    {
        let mut probed = None;
        count_hot_path(&format!("frame {}", case.label), || {
            probed = Some(compose(
                FrameContext {
                    doc,
                    env,
                    shaper: &shaper,
                    snap: &snap,
                    theme,
                },
                &FrameRequest {
                    viewport: (env.viewport_width, VIEWPORT_H),
                    scroll: 0.0,
                    cursor: Cursor { block, offset: 0 },
                    selection: None,
                    marked: None,
                    search_query: "",
                    search_skip: None,
                },
                &FallbackSolver,
                None,
            ));
        });
        let frame = probed.expect("the frame probe must compose a frame");
        eprintln!(
            "[frame {}] texts={} decorations={} cells={} atoms={} islands={}",
            case.label,
            frame.snapshot.texts.len(),
            frame.snapshot.decorations.len(),
            frame.snapshot.cells.len(),
            frame.snapshot.content_atoms_painted,
            frame.assembly.island_stats.islands_built
        );
    }
    cx.bench_iter(move |_cx| {
        let frame = compose(
            FrameContext {
                doc,
                env,
                shaper: &shaper,
                snap: &snap,
                theme,
            },
            &FrameRequest {
                viewport: (env.viewport_width, VIEWPORT_H),
                scroll: 0.0,
                cursor: Cursor { block, offset: 0 },
                selection: None,
                marked: None,
                search_query: "",
                search_skip: None,
            },
            &FallbackSolver,
            None,
        );
        assert!(
            !frame.texts.is_empty(),
            "a composed frame must paint at least one text piece"
        );
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/settle-cold", sample_size = 10)]
fn render_settle_cold(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(
            window,
            app,
            &case.theme,
            1.0,
            ShapeCache::new(),
            empty_media(),
        )
    });
    let doc = &case.doc;
    let env = case.env;
    let theme = &case.theme;
    {
        let mut probe = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
            0.0,
            VIEWPORT_H,
        );
        let p = probe.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        eprintln!(
            "[settle-cold {}] iterations={} resolved_top={:.1} stats={:?}",
            case.label,
            p.iterations,
            p.resolved_top,
            shaper.stats()
        );
    }
    cx.bench_iter(move |_cx| {
        let mut engine = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
            0.0,
            VIEWPORT_H,
        );
        let published = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        assert!(
            published.iterations >= 1,
            "settling an engine must run at least one iteration"
        );
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/settle-warm", sample_size = 10)]
fn render_settle_warm(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(
            window,
            app,
            &case.theme,
            1.0,
            ShapeCache::new(),
            empty_media(),
        )
    });
    let mut engine = IncrementalEngine::with_window(
        &case.doc.document,
        case.env,
        Estimator::from_theme(&case.theme),
        case.theme.layout_theme(),
        0.0,
        VIEWPORT_H,
    );
    engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
    let first = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
    eprintln!(
        "[settle-warm {}] iterations={} resolved_top={:.1} stats={:?}",
        case.label,
        first.iterations,
        first.resolved_top,
        shaper.stats()
    );
    cx.bench_iter(move |_cx| {
        let published = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        assert!(
            published.iterations >= 1,
            "settling an engine must run at least one iteration"
        );
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/settle-full", sample_size = 10)]
fn render_settle_full(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(
            window,
            app,
            &case.theme,
            1.0,
            ShapeCache::new(),
            empty_media(),
        )
    });
    let doc = &case.doc;
    let env = case.env;
    let theme = &case.theme;
    {
        let mut probe = IncrementalEngine::new(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
        );
        let p = probe.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        eprintln!(
            "[settle-full {}] iterations={} resolved_top={:.1}",
            case.label, p.iterations, p.resolved_top
        );
    }
    cx.bench_iter(move |_cx| {
        let mut engine = IncrementalEngine::new(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
        );
        let published = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        assert!(
            published.iterations >= 1,
            "settling an engine must run at least one iteration"
        );
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/settle-wide-window", sample_size = 10)]
fn render_settle_wide_window(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(
            window,
            app,
            &case.theme,
            1.0,
            ShapeCache::new(),
            empty_media(),
        )
    });
    let doc = &case.doc;
    let env = case.env;
    let theme = &case.theme;
    {
        let mut probe = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
            0.0,
            VIEWPORT_H * 10.0,
        );
        let p = probe.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        eprintln!(
            "[settle-wide {}] iterations={} resolved_top={:.1}",
            case.label, p.iterations, p.resolved_top
        );
    }
    cx.bench_iter(move |_cx| {
        let mut engine = IncrementalEngine::with_window(
            &doc.document,
            env,
            Estimator::from_theme(theme),
            theme.layout_theme(),
            0.0,
            VIEWPORT_H * 10.0,
        );
        let published = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        assert!(
            published.iterations >= 1,
            "settling an engine must run at least one iteration"
        );
    });
}

gpui::bench_group!(
    benches,
    content_shaper_new,
    content_artifact,
    content_artifact_cold,
    render_frame,
    render_settle_cold,
    render_settle_warm,
    render_settle_full,
    render_settle_wide_window
);
gpui::bench_main!(benches);
