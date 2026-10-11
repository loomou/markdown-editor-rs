use gpui::BenchAppContext;
use md_bench::gpui_support::{
    FrameCase, VIEWPORT_H, count_hot_path, empty_media, large_cases, mount_shell,
    render_frame_cases, text_nodes,
};
use md_bench::harness::first_leaf;
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_core::doc::Cursor;
use md_layout::assembly::assemble_shared_with;
use md_layout::island::FallbackSolver;
use md_render::frame::{FrameContext, FrameRequest, compose};
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};
use md_render::snap::SnapOperator;
use md_theme::{Density, LineBreakMode};
use std::cell::Cell;
use std::rc::Rc;

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/content-artifact", sample_size = 10)]
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
        "[large artifact {}] text-nodes={} stats={:?}",
        case.label,
        node_count,
        cache.stats()
    );
}

#[gpui::bench(inputs = render_frame_cases(), input_name = "case", group = "large/render-frame", sample_size = 10)]
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
        count_hot_path(&format!("large frame {}", case.label), || {
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
            "[large frame {}] texts={} cells={} atoms={} islands={}",
            case.label,
            frame.snapshot.texts.len(),
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

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/render-settle-cold", sample_size = 10)]
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

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/render-settle-warm", sample_size = 10)]
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
        "[large settle-warm {}] iterations={} resolved_top={:.1}",
        case.label, first.iterations, first.resolved_top
    );
    cx.bench_iter(move |_cx| {
        let published = engine.settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver);
        assert!(
            published.iterations >= 1,
            "settling an engine must run at least one iteration"
        );
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/render-settle-full", sample_size = 10)]
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

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/toggles/reading", sample_size = 10)]
fn reading(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let on = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !on.get();
        on.set(next);
        shell.bench_set_reading(next, cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/toggles/variant", sample_size = 10)]
fn variant(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    cx.bench_renderer(shell, |shell, _window, cx| {
        shell.bench_toggle_variant(cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/toggles/density", sample_size = 10)]
fn density(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let compact = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !compact.get();
        compact.set(next);
        let density = if next {
            Density::Compact
        } else {
            Density::Normal
        };
        shell.bench_set_density(density, cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/toggles/line-break", sample_size = 10)]
fn line_break(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let optimal = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !optimal.get();
        optimal.set(next);
        let mode = if next {
            LineBreakMode::Optimal
        } else {
            LineBreakMode::Greedy
        };
        shell.bench_set_line_break(mode, cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = render_frame_cases(), input_name = "case", group = "probe/frame-split-tree", sample_size = 10)]
fn frame_split_tree(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let doc = &case.doc;
    let theme = &case.theme;
    cx.bench_iter(move |_cx| {
        let tree = md_layout::compose::compose(&doc.document, &theme.layout_theme());
        assert!(
            !tree.nodes().is_empty(),
            "compose must produce a non-empty box tree"
        );
    });
}

#[gpui::bench(inputs = render_frame_cases(), input_name = "case", group = "probe/frame-split-assemble", sample_size = 10)]
fn frame_split_assemble(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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
    let tree = Rc::new(md_layout::compose::compose(
        &case.doc.document,
        &case.theme.layout_theme(),
    ));
    let env = case.env;
    cx.bench_iter(move |_cx| {
        let assembly = assemble_shared_with(Rc::clone(&tree), env, &shaper, &FallbackSolver, None);
        assert!(
            !assembly.heights.is_empty(),
            "the split assembly must resolve heights"
        );
    });
}

gpui::bench_group!(
    benches,
    content_artifact,
    render_frame,
    render_settle_cold,
    render_settle_warm,
    render_settle_full,
    reading,
    variant,
    density,
    line_break,
    frame_split_tree,
    frame_split_assemble
);
gpui::bench_main!(benches);
