use gpui::{BenchAppContext, Task};
use md_bench::gpui_support::{
    FrameCase, Step, VIEWPORT_H, build_engine, count_hot_path, editor_window_bounds, empty_media,
    large_cases, mount_shell, open_engine, run_keystroke, run_scroll,
};
use md_content::shaper::{GpuiShaper, ShapeCache};
use md_layout::island::FallbackSolver;
use md_render::incremental::ScrollAnchor;

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/cold-open", sample_size = 10)]
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
    let (top, bottom) = editor_window_bounds(0.0, VIEWPORT_H);
    let mut published = 0u32;
    count_hot_path(&format!("large cold-open {}", case.label), || {
        let mut engine = build_engine(case, top, bottom);
        published = engine
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
    eprintln!(
        "[large cold-open {}] leaves={} iterations={}",
        case.label,
        doc.text_leaves().len(),
        published
    );
    cx.bench_iter(move |_cx| {
        let mut engine = build_engine(case, top, bottom);
        let frame = engine
            .assemble_with_doc(
                &doc.document,
                ScrollAnchor::top(),
                VIEWPORT_H,
                &shaper,
                &FallbackSolver,
            )
            .1;
        assert!(frame.iterations >= 1, "a cold open must publish a frame");
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/frame", sample_size = 10)]
fn frame(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    cx.bench_renderer(shell, |_, _, cx| cx.notify());
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/keystroke", sample_size = 10)]
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
        count_hot_path(&format!("large keystroke {}", case.label), || {
            step = run_keystroke(&mut doc, &mut engine, &shaper, cursor);
        });
        eprintln!(
            "[large keystroke {}] iterations={} islands={} materialized={}",
            case.label,
            step.iterations,
            step.islands,
            engine.materialized_count()
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

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/scroll", sample_size = 10)]
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
        count_hot_path(&format!("large scroll {}", case.label), || {
            step = run_scroll(&doc, &mut engine, &shaper);
        });
        eprintln!(
            "[large scroll {}] iterations={} islands={} materialized={}",
            case.label,
            step.iterations,
            step.islands,
            engine.materialized_count()
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

gpui::bench_group!(benches, cold_open, frame, keystroke, scroll);
gpui::bench_main!(benches);
