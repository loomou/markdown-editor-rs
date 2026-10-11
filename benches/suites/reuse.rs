use gpui::BenchAppContext;
use md_bench::gpui_support::{FrameCase, VIEWPORT_H, cases, empty_media};
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_layout::island::FallbackSolver;
use md_render::incremental::{Estimator, IncrementalEngine, ScrollAnchor};

fn open(case: &FrameCase, shaper: &GpuiShaper) -> IncrementalEngine {
    let mut engine = IncrementalEngine::with_window(
        &case.doc.document,
        case.env,
        Estimator::from_theme(&case.theme),
        case.theme.layout_theme(),
        0.0,
        VIEWPORT_H,
    );
    engine.assemble_with_doc(
        &case.doc.document,
        ScrollAnchor::top(),
        VIEWPORT_H,
        shaper,
        &FallbackSolver,
    );
    engine
}

fn islands(engine: &mut IncrementalEngine, case: &FrameCase, shaper: &GpuiShaper) -> u64 {
    let (assembly, published) = engine.assemble_with_doc(
        &case.doc.document,
        ScrollAnchor::top(),
        VIEWPORT_H,
        shaper,
        &FallbackSolver,
    );
    assert!(
        published.iterations >= 1,
        "a steady-state frame must publish at least one iteration"
    );
    assembly.island_stats.islands_built
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "render/reuse", sample_size = 10)]
fn reuse(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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
    {
        let mut engine = IncrementalEngine::with_window(
            &case.doc.document,
            case.env,
            Estimator::from_theme(&case.theme),
            case.theme.layout_theme(),
            0.0,
            VIEWPORT_H,
        );
        let (cold, _) = engine.assemble_with_doc(
            &case.doc.document,
            ScrollAnchor::top(),
            VIEWPORT_H,
            &shaper,
            &FallbackSolver,
        );
        let cold_islands = cold.island_stats.islands_built;
        let warm_islands = islands(&mut engine, case, &shaper);
        eprintln!(
            "[reuse {}] islands cold={cold_islands} warm={warm_islands} materialized={} exact={}",
            case.label,
            engine.materialized_count(),
            engine.last_exact_count()
        );
        assert!(
            cold_islands >= 1,
            "a cold frame must solve at least one island"
        );
        assert_eq!(
            warm_islands, 0,
            "a repeated frame must reuse every island instead of re-solving them"
        );
    }
    let mut engine = open(case, &shaper);
    cx.bench_iter(move |_cx| {
        let reused = islands(&mut engine, case, &shaper);
        assert_eq!(
            reused, 0,
            "the steady-state frame stopped reusing islands, so the incremental path degenerated"
        );
    });
}

gpui::bench_group!(benches, reuse);
gpui::bench_main!(benches);
