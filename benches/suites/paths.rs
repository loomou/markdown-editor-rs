use gpui::{BenchAppContext, Task};
use md_bench::gpui_support::{
    FrameCase, Step, VIEWPORT_H, build_engine, cases, count_hot_path, editor_window_bounds,
    empty_media, open_engine, run_keystroke, run_scroll,
};
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_layout::island::FallbackSolver;
use md_render::incremental::ScrollAnchor;

#[gpui::bench(inputs = cases(), input_name = "case", group = "paths/cold-open", sample_size = 10)]
fn cold_open(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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

    for factor in [1.0, 2.0, 3.0, 10.0] {
        let (top, bottom) = (0.0, VIEWPORT_H * factor);
        let mut assemble = 0u32;
        count_hot_path(&format!("cold-open {} {factor}x build", case.label), || {
            let mut engine = build_engine(case, top, bottom);
            assemble = engine
                .assemble_with_doc(
                    &doc.document,
                    ScrollAnchor::top(),
                    VIEWPORT_H,
                    &shaper,
                    &FallbackSolver,
                )
                .1
                .iterations;
        });
        let mut settled = 0u32;
        count_hot_path(
            &format!("cold-open {} {factor}x settle", case.label),
            || {
                let mut engine = build_engine(case, top, bottom);
                settled = engine
                    .settle(ScrollAnchor::top(), VIEWPORT_H, &shaper, &FallbackSolver)
                    .iterations;
            },
        );
        eprintln!(
            "[cold-open {}] window={factor}x assemble_with_doc={assemble} settle={settled}",
            case.label
        );
    }

    let (top, bottom) = editor_window_bounds(0.0, VIEWPORT_H);
    cx.bench_iter(move |_cx| {
        let mut engine = build_engine(case, top, bottom);
        let published = engine
            .assemble_with_doc(
                &doc.document,
                ScrollAnchor::top(),
                VIEWPORT_H,
                &shaper,
                &FallbackSolver,
            )
            .1;
        assert!(
            published.iterations >= 1,
            "a cold open must publish at least one frame"
        );
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "paths/keystroke", sample_size = 10)]
fn keystroke(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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
        let (mut doc, mut engine, cursor) = open_engine(case, &shaper);
        let mut step = Step {
            iterations: 0,
            islands: 0,
        };
        count_hot_path(&format!("keystroke {}", case.label), || {
            step = run_keystroke(&mut doc, &mut engine, &shaper, cursor);
        });
        eprintln!(
            "[keystroke {}] iterations={} islands={} materialized={} exact={}",
            case.label,
            step.iterations,
            step.islands,
            engine.materialized_count(),
            engine.last_exact_count()
        );
    }
    cx.bench_batched_task(
        |_cx| open_engine(case, &shaper),
        |input, _cx| {
            let (doc, engine, cursor) = input;
            let step = run_keystroke(doc, engine, &shaper, *cursor);
            assert!(step.iterations >= 1, "a keystroke must publish a frame");
            Task::ready(())
        },
    );
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "paths/scroll", sample_size = 10)]
fn scroll(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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
        let (doc, mut engine, _) = open_engine(case, &shaper);
        let mut step = Step {
            iterations: 0,
            islands: 0,
        };
        count_hot_path(&format!("scroll {}", case.label), || {
            step = run_scroll(&doc, &mut engine, &shaper);
        });
        eprintln!(
            "[scroll {}] iterations={} islands={} materialized={} exact={}",
            case.label,
            step.iterations,
            step.islands,
            engine.materialized_count(),
            engine.last_exact_count()
        );
    }
    cx.bench_batched_task(
        |_cx| open_engine(case, &shaper),
        |input, _cx| {
            let (doc, engine, _) = input;
            let step = run_scroll(doc, engine, &shaper);
            assert!(step.iterations >= 1, "a scroll must publish a frame");
            Task::ready(())
        },
    );
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "paths/noop", sample_size = 10)]
fn noop(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
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
    cx.bench_batched_task(
        |_cx| open_engine(case, &shaper),
        |_input, _cx| Task::ready(()),
    );
}

gpui::bench_group!(benches, cold_open, keystroke, scroll, noop);
gpui::bench_main!(benches);
