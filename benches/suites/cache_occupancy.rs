use gpui::{BenchAppContext, Task};
use md_bench::corpus::{self, human};
use md_bench::gpui_support::{
    FrameCase, VIEWPORT_H, empty_media, large_cases, open_engine, text_nodes,
};
use md_bench::harness::{doc, document_theme, environment};
use md_content::shaper::{CacheStats, GpuiShaper, ShapeCache};
use md_core::Px;
use md_core::doc::Doc;
use md_layout::box_tree::{BoxTree, LayoutBoxId};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use md_render::incremental::IncrementalEngine;
use md_theme::DocumentTheme;
use std::fmt;

const TABLE_ROWS: usize = 2_400;
const TABLE_COLS: usize = 8;
const TABLE_SEED: u64 = 0x5eed_0cac;
const WIDTHS: [usize; 6] = [512, 1_024, 2_048, 4_096, 8_192, 16_384];
const FRAMES: usize = 8;

struct WidthCase {
    label: String,
    text: String,
    theme: DocumentTheme,
    env: BoxLayoutEnvironment,
    nodes: usize,
}

impl fmt::Display for WidthCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn width_cases() -> Vec<WidthCase> {
    let text = corpus::table(TABLE_ROWS, TABLE_COLS, TABLE_SEED);
    WIDTHS
        .into_iter()
        .map(|nodes| WidthCase {
            label: format!("nodes-{}", human(nodes)),
            text: text.clone(),
            theme: document_theme(),
            env: environment(),
            nodes,
        })
        .collect()
}

fn shape_all(
    tree: &BoxTree,
    nodes: &[LayoutBoxId],
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
) {
    for id in nodes {
        let node = tree.get(*id);
        shaper.artifact(
            tree.text_of(node),
            tree.runs_of(node),
            tree.content_width(*id, env.viewport_width),
            node.kind(),
            node.shape_ident(),
        );
    }
}

fn occupancy(cache: &ShapeCache) -> (usize, usize, CacheStats) {
    (cache.entry_count(), cache.byte_count(), cache.stats())
}

fn scroll_to(doc: &Doc, engine: &mut IncrementalEngine, shaper: &GpuiShaper, y: Px) -> u64 {
    let anchor = engine.anchor_at_y(y, shaper, &FallbackSolver);
    let (assembly, _) =
        engine.assemble_with_doc(&doc.document, anchor, VIEWPORT_H, shaper, &FallbackSolver);
    assembly.island_stats.islands_built
}

#[gpui::bench(inputs = width_cases(), input_name = "case", group = "cache/frame-width", sample_size = 10)]
fn frame_width(case: &WidthCase, cx: &mut BenchAppContext<'_, '_>) {
    let doc = doc(&case.text);
    let (tree, nodes) = text_nodes(&doc, &case.theme);
    assert!(
        nodes.len() >= case.nodes,
        "{}: the table corpus yielded {} text nodes, fewer than the {} requested",
        case.label,
        nodes.len(),
        case.nodes
    );
    let subset: Vec<LayoutBoxId> = nodes.iter().take(case.nodes).copied().collect();
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(window, app, &case.theme, 1.0, cache.clone(), empty_media())
    });
    let env = case.env;
    cache.begin_frame(shaper.env_fingerprint());
    shape_all(&tree, &subset, &shaper, env);
    cache.end_frame();
    let (entries, bytes, stats) = occupancy(&cache);
    eprintln!(
        "[cache/frame-width {}] corpus-nodes={} shaped={} entries={} bytes={} evictions={} misses={} same-frame-hits={}",
        case.label,
        nodes.len(),
        subset.len(),
        entries,
        bytes,
        stats.evictions,
        stats.misses,
        stats.same_frame_hits
    );
    assert!(
        entries > 0,
        "{}: shaping {} nodes must leave the cache non-empty",
        case.label,
        case.nodes
    );
    cx.bench_iter(move |_cx| {
        shape_all(&tree, &subset, &shaper, env);
    });
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "cache/scroll-session", sample_size = 10)]
fn scroll_session(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let cache = ShapeCache::new();
    let shaper = window.update(|window, app| {
        GpuiShaper::new(window, app, &case.theme, 1.0, cache.clone(), empty_media())
    });
    let fp = shaper.env_fingerprint();
    {
        let mut max_entries = 0usize;
        let mut max_bytes = 0usize;
        let mut islands = 0u64;
        cache.begin_frame(fp);
        let (doc, mut engine, _) = open_engine(case, &shaper);
        cache.end_frame();
        max_entries = max_entries.max(cache.entry_count());
        max_bytes = max_bytes.max(cache.byte_count());
        for frame in 1..=FRAMES {
            cache.begin_frame(fp);
            islands += scroll_to(&doc, &mut engine, &shaper, frame as Px * VIEWPORT_H);
            cache.end_frame();
            max_entries = max_entries.max(cache.entry_count());
            max_bytes = max_bytes.max(cache.byte_count());
        }
        let stats = cache.stats();
        eprintln!(
            "[cache/scroll-session {}] frames={} max-entries={} max-bytes={} final-entries={} frames-seen={} islands={} env-invalidations={} evictions={} misses={} cross-frame-hits={} same-frame-hits={}",
            case.label,
            FRAMES,
            max_entries,
            max_bytes,
            cache.entry_count(),
            stats.frames,
            islands,
            stats.env_invalidations,
            stats.evictions,
            stats.misses,
            stats.cross_frame_hits,
            stats.same_frame_hits
        );
        assert_eq!(
            stats.frames as usize,
            FRAMES + 1,
            "{}: the session must advance the frame counter once per frame",
            case.label
        );
        assert_eq!(
            stats.env_invalidations, 1,
            "{}: the environment must be established once and then stay stable",
            case.label
        );
        assert!(
            islands > 0,
            "{}: a scroll session must build new islands, or the frames are identical",
            case.label
        );
        assert!(
            stats.misses > 0,
            "{}: a scroll session must shape something",
            case.label
        );
        assert!(
            max_entries > 0,
            "{}: a scroll session must populate the shape cache",
            case.label
        );
    }
    let iter_cache = cache.clone();
    cx.bench_batched_task(
        |_cx| open_engine(case, &shaper),
        |(doc, engine, _), _cx| {
            for frame in 1..=FRAMES {
                iter_cache.begin_frame(fp);
                scroll_to(doc, engine, &shaper, frame as Px * VIEWPORT_H);
                iter_cache.end_frame();
            }
            Task::ready(())
        },
    );
}

gpui::bench_group!(benches, frame_width, scroll_session);
gpui::bench_main!(benches);
