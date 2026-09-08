use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use md_render::blocks::table::{CellRect, paint_cell_grid};
use md_render::search::{SearchScan, scan_document};
use md_theme::DocumentTheme;
use std::time::Instant;

#[test]
fn adversarial_search_samples() {
    let doc = Doc::new(load_markdown(&"a".repeat(30_000), editor_options()));
    for prefix in [250, 500, 1000] {
        let query = format!("{}b", "a".repeat(prefix));
        let started = Instant::now();
        let result = scan_document(&doc, &query);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "search hay=30000 query={} elapsed_ms={elapsed_ms:.3} result={result:?}",
            query.len()
        );
        assert_eq!(result, SearchScan::Empty);
        assert!(
            elapsed_ms < 50.0,
            "no-hit scan must stay linear: query={} took {elapsed_ms:.3}ms",
            query.len()
        );
    }
}

#[test]
fn grid_scaling_samples() {
    let theme = DocumentTheme::one_dark();
    for count in [800usize, 1600, 3200] {
        let cells: Vec<_> = (0..count)
            .map(|i| CellRect {
                table: 1,
                rect: (((i % 100) * 30) as f64, ((i / 100) * 24) as f64, 30.0, 24.0),
                is_header: i < 100,
            })
            .collect();
        let started = Instant::now();
        let ops = paint_cell_grid(&cells, &theme);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "grid cells={count} elapsed_ms={elapsed_ms:.3} ops={}",
            ops.len()
        );
        assert!(!ops.is_empty());
        assert!(
            elapsed_ms < 50.0,
            "grid construction must stay near-linear: {count} cells took {elapsed_ms:.3}ms"
        );
    }
}
