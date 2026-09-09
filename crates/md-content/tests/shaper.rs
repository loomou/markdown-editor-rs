use gpui::TestAppContext;
use md_content::shaper::{GpuiShaper, ShapeCache, ShapeMedia, ShapePart};
use md_core::block::BlockKind;
use md_core::inline::{InlineAlign, InlineMarks, InlineRun};
use md_layout::shaper::ShapeIdentity;
use md_theme::DocumentTheme;
use std::rc::Rc;

fn media() -> ShapeMedia {
    ShapeMedia {
        mermaid_fitted: Rc::new(Default::default()),
        math_metrics: Rc::new(Default::default()),
        math_gen: 0,
        image_sizes: Rc::new(Default::default()),
        image_failed: Rc::new(Default::default()),
        image_gen: 0,
        link_dests: Rc::new(Default::default()),
        link_raw: Rc::new(Default::default()),
        block_image_dest: Rc::new(Default::default()),
        block_code_lang: Rc::new(Default::default()),
    }
}

fn super_run(range: std::ops::Range<u32>) -> [InlineRun; 1] {
    [InlineRun {
        display_range: range,
        source_range: None,
        marks: InlineMarks::SUPER,
        link: None,
    }]
}

#[gpui::test]
fn band_mode_preserves_trailing_hard_breaks(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media());

        let plain = shaper.artifact(
            "a\n\n",
            &[],
            400.0,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        assert_eq!(plain.rows, 3, "plain path: one new row per newline");

        let runs = super_run(0..1);
        let art = shaper.artifact(
            "a\n\n",
            &runs,
            400.0,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        assert_eq!(
            art.rows, 3,
            "band path: the blank row after the final hard newline must not be lost (newline count + 1)"
        );
        let (_, row) = shaper.position_for_offset(&art, "a\n\n".len(), InlineAlign::Start, 400.0);
        assert_eq!(row, 2, "the caret at end of text must land on that last blank row (0-based)");

        let art = shaper.artifact(
            "a",
            &runs,
            400.0,
            BlockKind::Paragraph,
            ShapeIdentity {
                index: 2,
                ..Default::default()
            },
        );
        assert_eq!(art.rows, 1, "no trailing newline, no blank row to append");
    });
}

#[gpui::test]
fn band_wrapped_row_start_round_trips(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media());
        let text = "one two three four five six x";
        let runs = [InlineRun {
            display_range: (text.len() as u32 - 1)..text.len() as u32,
            source_range: None,
            marks: InlineMarks::SUPER,
            link: None,
        }];
        let art = shaper.artifact(
            text,
            &runs,
            75.0,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        assert!(
            art.bands.len() > 1,
            "premise: wrapping really produced multiple bands"
        );
        let seam = match &art.bands[1].parts[0] {
            ShapePart::Text { byte_start, .. } => *byte_start,
            _ => panic!("the fixture's second band should start with text"),
        };

        let hit = shaper.offset_for_position(&art, 0.0, 1, InlineAlign::Start, 75.0);
        assert_eq!(
            hit, seam,
            "clicking at the row start hits the seam offset (premise: the forward mapping is right)"
        );

        let position = shaper.position_for_offset(&art, hit, InlineAlign::Start, 75.0);
        assert_eq!(
            position.1, 1,
            "clicking the row start of a wrapped row must keep the caret on that row"
        );

        let mid = seam + 1;
        let position = shaper.position_for_offset(&art, mid, InlineAlign::Start, 75.0);
        assert_eq!(position.1, 1, "an in-row offset keeps its row assignment");
    });
}

#[gpui::test]
fn mixed_paragraph_scaling_stays_near_linear(cx: &mut TestAppContext) {
    fn shape_mixed_paragraph(
        window: &gpui::Window,
        app: &gpui::App,
        theme: &md_theme::DocumentTheme,
        count: usize,
    ) -> (usize, f64) {
        let shaper = GpuiShaper::new(window, app, theme, 1.0, ShapeCache::new(), media());
        let text = format!("{}x", "word ".repeat(count));
        let mut runs: Vec<_> = (0..count)
            .map(|n| InlineRun {
                display_range: (n * 5) as u32..(n * 5 + 4) as u32,
                source_range: None,
                marks: if n % 2 == 0 {
                    InlineMarks::EM
                } else {
                    InlineMarks::STRONG
                },
                link: None,
            })
            .collect();
        runs.push(InlineRun {
            display_range: (text.len() - 1) as u32..text.len() as u32,
            source_range: None,
            marks: InlineMarks::SUPER,
            link: None,
        });
        let start = std::time::Instant::now();
        let art = shaper.artifact(
            &text,
            &runs,
            700.0,
            BlockKind::Paragraph,
            Default::default(),
        );
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "band styled_words={count}, bytes={}, bands={}, elapsed_ms={elapsed_ms:.3}",
            text.len(),
            art.bands.len()
        );
        assert!(!art.bands.is_empty());
        (text.len(), elapsed_ms)
    }

    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let (base_bytes, base_ms) = shape_mixed_paragraph(window, app, &theme, 1000);
        let (_, mid_ms) = shape_mixed_paragraph(window, app, &theme, 2000);
        let (top_bytes, top_ms) = shape_mixed_paragraph(window, app, &theme, 4000);
        assert_eq!(
            top_bytes - base_bytes,
            15_000,
            "the step layout must be a clean 4x relation"
        );
        let ratio = top_ms / base_ms.max(0.001);
        println!("scaling ratio 4000/1000 = {ratio:.2} (mid {mid_ms:.3}ms)");
        assert!(
            ratio < 8.0,
            "layout of mixed words and styled spans is still superlinear: 4000 words run {ratio:.1}x \
             slower than 1000 (linear would be ~4x, quadratic ~16x)"
        );
    });
}

#[gpui::test]
fn long_token_keeps_last_wrap_row_open(cx: &mut gpui::TestAppContext) {
    fn band_row_widths(art: &md_content::shaper::ShapeArtifact) -> Vec<f64> {
        art.bands
            .iter()
            .map(|band| {
                band.parts
                    .iter()
                    .map(|part| match part {
                        ShapePart::Text { line, x, .. } => *x + f32::from(line.width()) as f64,
                        ShapePart::Math { x, width, .. } | ShapePart::Image { x, width, .. } => {
                            *x + *width
                        }
                    })
                    .fold(0.0, f64::max)
            })
            .collect()
    }

    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media());
        let text = "abcdefghijklmnopq x";
        let runs = super_run((text.len() - 1) as u32..text.len() as u32);
        let art = shaper.artifact(
            text,
            &runs,
            100.0,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        let widths = band_row_widths(&art);
        println!("long token rows={}, widths={widths:?}", art.rows);
        assert_eq!(
            art.rows, 2,
            "the last short row after wrapping a long token must keep receiving what follows"
        );
    });
}

#[gpui::test]
fn multiline_script_retains_its_first_line(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media());

        let doc = md_core::document::load_markdown(
            "before ^first\nsecond^ after\n",
            md_core::document::editor_options(),
        );
        let block = doc.text_leaves()[0];
        let node = doc.live_id(block).unwrap();
        let text = doc.text_of(block).unwrap();
        assert_eq!(text, "before first\nsecond after");
        assert!(
            doc.runs(node).iter().any(|run| run.marks.is_script()),
            "fixture: the script run must span the hard line"
        );
        let art = shaper.artifact(
            text,
            doc.runs(node),
            800.0,
            doc.kind(block).unwrap(),
            ShapeIdentity::default(),
        );
        let retained: String = art
            .bands
            .iter()
            .flat_map(|band| &band.parts)
            .filter_map(|part| match part {
                ShapePart::Text { line, .. } => Some(line.text.as_ref()),
                _ => None,
            })
            .collect();
        eprintln!("retained={retained:?}, rows={}", art.rows);
        assert_eq!(retained, text.replace('\n', ""));
        assert!(
            art.rows >= 2,
            "a hard line inside the atom must break the row"
        );
    });
}

#[gpui::test]
fn multiline_subscript_retains_its_first_line(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let theme = DocumentTheme::one_dark();
        let shaper = GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media());

        let doc = md_core::document::load_markdown(
            "before ~first\nsecond~ after\n",
            md_core::document::editor_options(),
        );
        let block = doc.text_leaves()[0];
        let node = doc.live_id(block).unwrap();
        let text = doc.text_of(block).unwrap();
        let art = shaper.artifact(
            text,
            doc.runs(node),
            800.0,
            doc.kind(block).unwrap(),
            ShapeIdentity::default(),
        );
        let retained: String = art
            .bands
            .iter()
            .flat_map(|band| &band.parts)
            .filter_map(|part| match part {
                ShapePart::Text { line, .. } => Some(line.text.as_ref()),
                _ => None,
            })
            .collect();
        eprintln!("retained={retained:?}, rows={}", art.rows);
        assert_eq!(retained, text.replace('\n', ""));
    });
}
