use super::support::{
    CountingMeasure, HalfMeasure, dummy_layout, estimator, loaded, long_doc, tall_doc,
};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use crate::incremental::store::EvictionPolicy;
use md_core::Px;
use md_core::document::{editor_options, load_markdown};
use md_layout::compose::LayoutTheme;
use md_layout::compose::compose;
use md_layout::flow::HeightState;
use md_layout::island::FallbackSolver;
use md_layout::spine::{FlowItemKind, FlowSpine};
use md_layout::style::{BoxDisplay, BoxLayoutEnvironment, BoxLayoutStyle, Edges};
use std::cell::Cell;
use std::fmt::Write;

fn keep_screens(engine: &IncrementalEngine) -> f64 {
    match engine.eviction {
        EvictionPolicy::Windowed { keep_screens } => keep_screens,
        EvictionPolicy::Unbounded => 0.0,
    }
}

fn warm_content_count(engine: &IncrementalEngine, top: Px, vh: Px) -> usize {
    let pad = keep_screens(engine) * vh;
    let lo = (top - pad).max(0.0);
    let hi = top + vh + pad;
    engine
        .spine
        .visible(lo, hi)
        .filter(|&pos| engine.spine.item_at(pos).is_content())
        .count()
}

#[test]
fn offscreen_text_edit_does_not_measure() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(measure.calls.get() > 0);
    let last = *doc.text_leaves().last().expect("leaf");
    measure.calls.set(0);
    let changes = doc.replace_text(last, 0..0, "Z");
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(measure.calls.get(), 0);
}

#[test]
fn visible_text_edit_remeasures() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves().into_iter().next().expect("leaf");
    measure.calls.set(0);
    let changes = doc.replace_text(first, 0..0, "Z");
    engine.apply_changes(&doc, &changes);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(measure.calls.get() > 0);
}

#[test]
fn scroll_and_type_do_not_reflatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    engine.assemble_incremental(sa, 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let last = *doc.text_leaves().last().expect("leaf");
    let changes = doc.replace_text(last, 0..0, "Z");
    engine.apply_changes(&doc, &changes);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
}

#[test]
fn visible_work_does_not_scale_with_document() {
    fn n_paras(n: usize) -> md_core::document::Document {
        let mut md = String::new();
        for i in 0..n {
            let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
        }
        load_markdown(&md, editor_options())
    }
    let env = BoxLayoutEnvironment::default();
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let mut a = IncrementalEngine::new(&n_paras(80), env, estimator(), dummy_layout());
    let (wa, _) = a.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let mut b = IncrementalEngine::new(&n_paras(320), env, estimator(), dummy_layout());
    let (wb, _) = b.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let va = wa.window.as_ref().expect("window a").items_visited;
    let vb = wb.window.as_ref().expect("window b").items_visited;
    assert!(va > 0);
    assert!(vb > 0);
    assert!(
        vb < va * 2 + 32,
        "visible items {vb} should not track 4x document vs {va}"
    );
}

#[test]
fn published_geometries_are_window_only() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let (top_asm, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let top_published = top_asm.geometries.len();
    assert!(top_published > 0);
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    let (assembly, published) = engine.assemble_incremental(sa, 600.0, &measure, &solver);
    let copied = assembly.geometries.len();
    let stored = engine.store.materialized_count();
    assert!(copied > 0);
    assert!(
        stored >= copied,
        "store {stored} should cover the published window {copied}"
    );
    let warm = warm_content_count(&engine, published.resolved_top, 600.0);
    assert!(
        stored <= warm + 8,
        "store {stored} escaped the warm window (protected {warm})"
    );
    assert_eq!(engine.publish_gen(), 2);
}

#[test]
fn scrolling_does_not_accumulate_offscreen_exact() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    let first = engine.store.materialized_count();
    assert!(first > 0);
    let mut peak = first;
    for y in [800.0, 1_600.0, 2_400.0, 3_200.0] {
        let sa = engine.anchor_at_y(y, &measure, &solver);
        let (_, published) = engine.assemble_incremental(sa, vh, &measure, &solver);
        let n = engine.store.materialized_count();
        peak = peak.max(n);
        let warm = warm_content_count(&engine, published.resolved_top, vh);
        assert!(
            n <= warm + 8,
            "y={y}: store {n} escaped the warm window (protected {warm})"
        );
    }
    assert!(
        peak < first * 8,
        "store grew with scroll distance: first={first} peak={peak}"
    );
    assert!(
        engine.store.last_exact_count() > 0,
        "offscreen exact should degrade to last_exact, not vanish"
    );
}

#[test]
fn eviction_keeps_total_height_and_scroll_y() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    let (_, far) = engine.assemble_incremental(sa, vh, &measure, &solver);
    let height = engine.total_height();
    let y = far.resolved_top;
    engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    let sa = engine.anchor_at_y(y, &measure, &solver);
    let (_, back) = engine.assemble_incremental(sa, vh, &measure, &solver);
    assert!(
        (engine.total_height() - height).abs() < 1e-6,
        "eviction changed total height: {height} -> {}",
        engine.total_height()
    );
    assert!(
        (back.resolved_top - y).abs() < 1e-6,
        "return scroll y jumped: {y} -> {}",
        back.resolved_top
    );
}

#[test]
fn wide_table_visible_rows_stay_exact() {
    let mut md = String::from("| a | b |\n| --- | --- |\n");
    for i in 0..300 {
        let _ = writeln!(md, "| {i} | x |");
    }
    let doc = loaded(&md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 20_000.0;
    let (_, published) = engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    let mut content = 0usize;
    for pos in engine
        .spine
        .visible(published.resolved_top, published.resolved_top + vh)
    {
        if !engine.spine.item_at(pos).is_content() {
            continue;
        }
        content += 1;
        assert!(
            engine.spine.effective_height(pos).is_exact(),
            "visible content at {pos} was evicted"
        );
    }
    assert!(
        content > 256,
        "fixture must put more than 256 rows on screen, got {content}"
    );
    assert_eq!(engine.store.materialized_count(), content);
}

#[test]
fn eviction_policy_default_is_windowed_not_unbounded() {
    assert_ne!(EvictionPolicy::default(), EvictionPolicy::unbounded());
    assert_eq!(EvictionPolicy::default(), EvictionPolicy::windowed(2.0));
}

#[test]
fn invalidated_geometry_falls_back_to_last_exact_height() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let (assembly, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let (&box_id, geometry) = assembly.geometries.iter().next().expect("visible geometry");
    let exact = geometry.border_box_height;

    assert!(engine.store.invalidate(box_id));
    assert!(engine.store.get(box_id).is_none());
    assert_eq!(engine.store.last_exact_height(box_id), Some(exact));
    assert_eq!(engine.estimate_island(box_id), exact);

    engine.materialize_one(box_id, &measure, &solver);
    assert!(
        engine
            .store
            .is_fresh(&engine.tree, box_id, engine.viewport_width())
    );
    assert_eq!(engine.store.last_exact_height(box_id), None);
}

#[test]
fn width_change_is_lazy_epoch() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    engine.assemble_incremental(sa, 600.0, &measure, &solver);
    let stored = engine.store.materialized_count();
    let height_before = engine.spine.total_height();
    let off = sa.item;
    let off_pos = engine.spine.location(off).expect("offscreen");
    assert!(engine.spine.item_at(off_pos).is_content());
    assert!(engine.spine.effective_height(off_pos).is_exact());
    engine.set_viewport_width(env.viewport_width + 120.0);
    assert_eq!(engine.flatten_gens, 1);
    assert_eq!(engine.store.materialized_count(), stored);
    assert_eq!(engine.spine.total_height(), height_before);
    assert!(engine.spine.estimated_count() > 0);
    assert!(engine.spine.item_at(off_pos).height.is_exact());
    assert!(!engine.spine.effective_height(off_pos).is_exact());
    let solves = engine.store.solve_calls;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    assert!(engine.store.solve_calls > solves);
    assert!(
        engine.store.materialized_count() < stored + 64,
        "width change rematerialized the document"
    );
    assert!(!engine.spine.effective_height(off_pos).is_exact());
}

#[test]
fn visible_fresh_store_repairs_stale_spine_epoch_before_publish() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let solves = engine.store.solve_calls;

    engine.spine.invalidate_layout_epoch();
    let (_, published) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    assert_eq!(engine.store.solve_calls, solves);
    let visible = engine
        .spine
        .visible(published.resolved_top, published.resolved_top + 600.0);
    for pos in visible {
        if engine.spine.item_at(pos).is_content() {
            assert!(
                engine.spine.effective_height(pos).is_exact(),
                "visible content at {pos} published with an estimated height"
            );
        }
    }
}

#[test]
fn incremental_assembly_publishes_and_resets_island_stats() {
    let doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;

    let (first, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(first.island_stats.islands_built > 0);

    let (second, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(second.island_stats.islands_built, 0);
}

#[test]
fn anchor_keeps_requested_y_when_estimate_ran_tall() {
    let doc = tall_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = HalfMeasure;
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);

    let mut drift = Vec::new();
    for _ in 0..3 {
        let mut want = None;
        for pos in 0..engine.spine.len() {
            let item = *engine.spine.item_at(pos);
            if !item.is_content() || engine.spine.effective_height(pos).is_exact() {
                continue;
            }
            let Some(top) = engine.spine.item_top(item.id) else {
                continue;
            };
            let h = item.height.px();
            if top < vh || h < 40.0 {
                continue;
            }
            want = Some(top + h * 0.9);
            break;
        }
        let Some(want) = want else {
            panic!("no candidate piece overestimates height; the fixture is broken");
        };
        let sa = engine.anchor_at_y(want, &measure, &solver);
        let (_asm, published) = engine.assemble_incremental(sa, vh, &measure, &solver);
        drift.push(published.resolved_top - want);
    }
    assert!(
        drift.iter().all(|d| d.abs() < 1e-6),
        "the viewport y drifted from the request: {drift:?}"
    );
}

#[test]
fn flatten_passes_avail_matching_ancestor_walk() {
    let doc = load_markdown("> outer\n>\n> > inner padded quote\n", editor_options());
    let layout = LayoutTheme::from_resolver(|_| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges::ZERO,
        padding: Edges::vh(0.0, 10.0),
        border: Edges {
            left: 4.0,
            ..Edges::ZERO
        },
        gap: 0.0,
    });
    let tree = compose(&doc, &layout);
    let vw = 800.0;
    let spine = FlowSpine::flatten(&tree, vw, &|id, avail| {
        let walk = tree.avail_width(id, vw);
        assert!(
            (avail - walk).abs() < 1e-9,
            "{id:?} passed={avail} walk={walk}"
        );
        HeightState::Estimated(20.0)
    });
    assert!(spine.len() > 1);
}

fn nested_quotes(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "> > quote {i} with some words to measure\n");
    }
    load_markdown(&md, editor_options())
}

fn visible_content_is_exact(engine: &IncrementalEngine, top: Px, vh: Px) -> bool {
    engine.spine.visible(top, top + vh).all(|pos| {
        let item = engine.spine.item_at(pos);
        match item.kind {
            FlowItemKind::Content { .. } => engine.spine.effective_height(pos).is_exact(),
            FlowItemKind::Collapsed { .. } => false,
            _ => true,
        }
    })
}

#[test]
fn scrolling_away_recollapses_quotes_and_scroll_back_is_exact() {
    let doc = nested_quotes(200);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;

    engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    let n_top = engine.spine.len();
    assert!(n_top > 1);

    let mut y: Px = 0.0;
    let mut peak = n_top;
    let mut sa = ScrollAnchor::top();
    for _ in 0..24 {
        y += 480.0;
        let dest = y.min(engine.spine.total_height() - 1.0).max(0.0);
        sa = engine.anchor_at_y(dest, &measure, &solver);
        engine.assemble_incremental(sa, vh, &measure, &solver);
        peak = peak.max(engine.spine.len());
    }
    let h_far = engine.spine.total_height();
    for _ in 0..48 {
        engine.assemble_incremental(sa, vh, &measure, &solver);
    }
    assert_eq!(
        engine.spine.total_height(),
        h_far,
        "idle recollapse must not change total height"
    );
    let n_far = engine.spine.len();
    assert!(
        n_far < n_top * 3 + 128,
        "spine grew with scroll distance: top={n_top} peak={peak} far={n_far}"
    );

    let (_asm, published) = engine.assemble_incremental(ScrollAnchor::top(), vh, &measure, &solver);
    assert!(
        visible_content_is_exact(&engine, published.resolved_top, vh),
        "first frame after scrolling back must publish Exact islands"
    );
}
