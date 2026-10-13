use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use md_render::blocks::table::{CellRect, paint_cell_grid};
use md_render::search::{SearchScan, scan_document};
use md_theme::DocumentTheme;
use std::time::Instant;

#[test]
fn adversarial_search_samples() {
    let doc = Doc::new(load_markdown(&"a".repeat(30_000), editor_options()));
    let mut samples = Vec::new();
    for prefix in [250, 500, 1000] {
        let query = format!("{}b", "a".repeat(prefix));
        let mut best_ms = f64::INFINITY;
        let mut result = SearchScan::Empty;
        for _ in 0..5 {
            let started = Instant::now();
            result = scan_document(&doc, &query);
            best_ms = best_ms.min(started.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "search hay=30000 query={} best_ms={best_ms:.3} result={result:?}",
            query.len()
        );
        assert_eq!(result, SearchScan::Empty);
        samples.push((query.len(), best_ms));
    }
    let (base_len, base_ms) = samples[0];
    let (top_len, top_ms) = *samples.last().unwrap();
    let ratio = top_ms / base_ms.max(0.001);
    assert!(
        ratio < 2.0,
        "a no-hit scan must not rescan per query char: query {top_len} is {ratio:.1}x the time of \
         query {base_len} (a rescanning matcher would be ~{:.1}x)",
        top_len as f64 / base_len as f64
    );
    assert!(
        top_ms < 20.0,
        "a no-hit scan must stay cheap: query {top_len} took {top_ms:.3}ms"
    );
}

#[test]
fn grid_scaling_samples() {
    let theme = DocumentTheme::one_dark();
    let mut samples = Vec::new();
    for count in [800usize, 1600, 3200] {
        let cells: Vec<_> = (0..count)
            .map(|i| CellRect {
                table: 1,
                rect: (((i % 100) * 30) as f64, ((i / 100) * 24) as f64, 30.0, 24.0),
                is_header: i < 100,
                corners: md_render::snapshot::TableCorners::default(),
            })
            .collect();
        let mut best_ms = f64::INFINITY;
        let mut ops_len = 0;
        for _ in 0..5 {
            let started = Instant::now();
            let ops = paint_cell_grid(&cells, &theme);
            best_ms = best_ms.min(started.elapsed().as_secs_f64() * 1000.0);
            ops_len = ops.len();
        }
        println!("grid cells={count} best_ms={best_ms:.3} ops={ops_len}");
        assert!(ops_len > 0);
        samples.push((count, best_ms));
    }
    let (base_count, base_ms) = samples[0];
    let (top_count, top_ms) = *samples.last().unwrap();
    let ratio = top_ms / base_ms.max(0.001);
    let step = top_count as f64 / base_count as f64;
    assert!(
        ratio < 2.0 * step,
        "grid construction must stay near-linear: {top_count} cells are {ratio:.1}x the time of \
         {base_count} (linear = {step:.1}x, quadratic = {:.1}x)",
        step * step
    );
    assert!(
        top_ms < 40.0,
        "grid construction must stay cheap: {top_count} cells took {top_ms:.3}ms"
    );
}
