use gpui::TestAppContext;
use gpui::WrappedLine;
use md_content::shaper::{GpuiShaper, ShapeArtifact, ShapeCache, ShapeMedia};
use md_core::block::BlockKind;
use md_core::inline::InlineAlign;
use md_layout::shaper::ShapeIdentity;
use md_theme::{DocumentTheme, LineBreakMode};
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

fn shaper_for(window: &gpui::Window, app: &gpui::App, mode: LineBreakMode) -> GpuiShaper {
    let mut theme = DocumentTheme::one_dark();
    theme.line_break.mode = mode;
    GpuiShaper::new(window, app, &theme, 1.0, ShapeCache::new(), media())
}

fn seam_bytes(line: &WrappedLine) -> impl Iterator<Item = usize> + '_ {
    line.wrap_boundaries.iter().map(|b| {
        line.unwrapped_layout
            .runs
            .get(b.run_ix)
            .and_then(|run| run.glyphs.get(b.glyph_ix))
            .map_or(0, |glyph| glyph.index)
    })
}

fn seams(art: &ShapeArtifact) -> Vec<usize> {
    art.lines.iter().flat_map(seam_bytes).collect()
}

fn rows(art: &ShapeArtifact) -> Vec<String> {
    let mut out = Vec::new();
    for line in &art.lines {
        let text: &str = line.text.as_ref();
        let mut cuts: Vec<usize> = std::iter::once(0).chain(seam_bytes(line)).collect();
        cuts.push(text.len());
        for pair in cuts.windows(2) {
            out.push(text[pair[0]..pair[1]].to_owned());
        }
    }
    out
}

const CHINESE: &str = "这是一个很长的中文段落，它包含「引号」、括号（以及）标点：需要正确地避头尾排版，不能把标点丢到行首，也不能把开括号留在行尾！";
const LATIN: &str = "one two three four five six seven eight nine ten eleven twelve";
const REPEATED: &str =
    "四十五个字符的中文内容在这里重复出现以便换行测试，再看一次四十五个字符的中文内容。";
const GREEK: &str = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
const SHORT: &str = "短句";

fn samples() -> [(&'static str, BlockKind, f64, &'static [usize]); 6] {
    [
        (
            CHINESE,
            BlockKind::Paragraph,
            90.0,
            &[27, 54, 81, 108, 135, 162],
        ),
        (
            LATIN,
            BlockKind::Paragraph,
            90.0,
            &[8, 14, 19, 28, 34, 40, 49, 56],
        ),
        (
            LATIN,
            BlockKind::Heading(1),
            110.0,
            &[4, 8, 13, 14, 19, 24, 28, 33, 34, 39, 40, 45, 49, 54, 56, 61],
        ),
        (REPEATED, BlockKind::Paragraph, 120.0, &[36, 72, 108]),
        (
            GREEK,
            BlockKind::Paragraph,
            75.0,
            &[6, 11, 17, 23, 30, 36, 40, 46, 51],
        ),
        (SHORT, BlockKind::Paragraph, 40.0, &[]),
    ]
}

fn plan(
    window: &gpui::Window,
    app: &gpui::App,
    mode: LineBreakMode,
    text: &str,
    kind: BlockKind,
    width: f64,
) -> Rc<ShapeArtifact> {
    shaper_for(window, app, mode).artifact(text, &[], width, kind, ShapeIdentity::default())
}

#[gpui::test]
fn greedy_mode_breaks_exactly_where_it_always_did(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for (text, kind, width, want) in samples() {
            let art = plan(window, app, LineBreakMode::Greedy, text, kind, width);
            assert_eq!(
                seams(&art),
                want,
                "the greedy path moved for {kind:?} at {width}px: {text:?}"
            );
        }
    });
}

#[gpui::test]
fn optimal_mode_is_a_different_plan_than_greedy(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let mut differed = 0;
        for (text, kind, width, _) in samples() {
            let greedy = plan(window, app, LineBreakMode::Greedy, text, kind, width);
            let optimal = plan(window, app, LineBreakMode::Optimal, text, kind, width);
            if seams(&greedy) != seams(&optimal) {
                differed += 1;
            }
        }
        assert!(
            differed >= 3,
            "the engine only disagreed with greedy on {differed} of the samples, so the new path is probably not wired up"
        );
    });
}

#[gpui::test]
fn optimal_mode_keeps_punctuation_off_the_line_edges(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for width in [60.0, 75.0, 90.0, 110.0, 140.0] {
            let art = plan(
                window,
                app,
                LineBreakMode::Optimal,
                CHINESE,
                BlockKind::Paragraph,
                width,
            );
            let rows = rows(&art);
            assert!(
                rows.len() > 1,
                "premise: {width}px is narrow enough to wrap"
            );
            for (i, row) in rows.iter().enumerate() {
                let first = row.chars().next().expect("a row is never empty");
                assert!(
                    !"，。、；：？！）」』】》".contains(first),
                    "at {width}px row {i} starts with a closing mark: {row:?}"
                );
                let last = row.chars().next_back().expect("a row is never empty");
                assert!(
                    !"\u{ff08}\u{300c}\u{300e}\u{3010}\u{300a}\u{201c}".contains(last),
                    "at {width}px row {i} ends with an opening mark: {row:?}"
                );
            }
        }
    });
}

#[gpui::test]
fn optimal_rows_cover_every_boundary(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for (text, kind, width, _) in samples() {
            let art = plan(window, app, LineBreakMode::Optimal, text, kind, width);
            let found = seams(&art);
            assert!(
                found.windows(2).all(|pair| pair[0] < pair[1]),
                "the seams must strictly increase: {found:?}"
            );
            assert!(
                found.last().is_none_or(|last| *last < text.len()),
                "a seam may never point past the text: {found:?}"
            );
            assert_eq!(
                art.rows as usize,
                rows(&art).len(),
                "rows must be Σ(boundaries + 1) for {kind:?} at {width}px"
            );
            assert_eq!(
                art.rows,
                art.lines
                    .iter()
                    .map(|line| line.wrap_boundaries.len() as u32 + 1)
                    .sum::<u32>(),
                "the artifact row count drifted from its own row map"
            );
        }
    });
}

#[gpui::test]
fn optimal_line_seams_round_trip(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let width = 90.0;
        let shaper = shaper_for(window, app, LineBreakMode::Optimal);
        let art = shaper.artifact(
            CHINESE,
            &[],
            width,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        assert!(
            art.rows >= 3,
            "premise: the paragraph wrapped over several rows"
        );
        for row in 1..art.rows {
            let start = shaper.offset_for_position(&art, 0.0, row, InlineAlign::Start, width);
            assert!(start > 0, "row {row} must start past the first row");
            let caret = shaper.caret_position_for_offset(&art, start, InlineAlign::Start, width);
            assert_eq!(caret.1, row, "the caret on the seam belongs to row {row}");
            assert_eq!(caret.0, 0.0, "the caret on the seam sits at the left edge");
            let selection = shaper.position_for_offset(&art, start, InlineAlign::Start, width);
            assert_eq!(
                selection.1,
                row - 1,
                "a selection ending on the seam still ends the row above it"
            );
            let again = shaper.offset_for_position(&art, 0.0, row, InlineAlign::Start, width);
            assert_eq!(
                again, start,
                "the seam must be stable across a second lookup"
            );
        }
    });
}

#[gpui::test]
fn optimal_mode_keeps_hard_lines_apart(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let text = format!("{LATIN}\n{LATIN}");
        let art = plan(
            window,
            app,
            LineBreakMode::Optimal,
            &text,
            BlockKind::Paragraph,
            90.0,
        );
        assert_eq!(art.lines.len(), 2, "a newline still starts a new hard line");
        for line in &art.lines {
            assert!(
                !line.wrap_boundaries.is_empty(),
                "each hard line wrapped on its own"
            );
            let end = line.text.len();
            assert!(
                seam_bytes(line).all(|seam| seam < end),
                "a seam of one hard line may not reach into the next"
            );
        }
    });
}

#[gpui::test]
fn code_blocks_never_take_the_optimal_path(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for kind in [BlockKind::CodeBlock, BlockKind::MetadataBlock] {
            let art = plan(window, app, LineBreakMode::Optimal, LATIN, kind, 60.0);
            assert!(
                art.lines.iter().all(|line| line.wrap_boundaries.is_empty()),
                "{kind:?} must not wrap in either mode"
            );
        }
    });
}

#[gpui::test]
fn the_mode_is_part_of_the_environment_fingerprint(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let greedy = shaper_for(window, app, LineBreakMode::Greedy).env_fingerprint();
        let optimal = shaper_for(window, app, LineBreakMode::Optimal).env_fingerprint();
        assert_ne!(
            greedy, optimal,
            "a mode change must invalidate the shape cache"
        );
    });
}
