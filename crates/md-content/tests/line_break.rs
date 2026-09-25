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

fn glyphs(line: &WrappedLine) -> Vec<(usize, f32)> {
    line.unwrapped_layout
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|glyph| (glyph.index, f32::from(glyph.position.x)))
        })
        .collect()
}

fn row_bounds(line: &WrappedLine) -> Vec<(usize, usize)> {
    let mut cuts = vec![0usize];
    for boundary in &line.wrap_boundaries {
        let before: usize = line
            .unwrapped_layout
            .runs
            .iter()
            .take(boundary.run_ix)
            .map(|run| run.glyphs.len())
            .sum();
        cuts.push(before + boundary.glyph_ix);
    }
    cuts.push(glyphs(line).len());
    cuts.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

fn row_ink(line: &WrappedLine, row: usize) -> f32 {
    let all = glyphs(line);
    let (start, end) = row_bounds(line)[row];
    let text: &str = line.text.as_ref();
    let last = (start..end)
        .rev()
        .find(|&at| {
            !text[all[at].0..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
        })
        .expect("the samples never end a row on whitespace alone");
    let right = all
        .get(last + 1)
        .map_or(f32::from(line.unwrapped_layout.width), |&(_, x)| x);
    right - all[start].1
}

fn positions(art: &ShapeArtifact) -> Vec<Vec<(usize, f32)>> {
    art.lines.iter().map(glyphs).collect()
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

fn seams_round_trip(window: &gpui::Window, app: &gpui::App, mode: LineBreakMode, width: f64) {
    let shaper = shaper_for(window, app, mode);
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
}

#[gpui::test]
fn optimal_line_seams_round_trip(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| seams_round_trip(window, app, LineBreakMode::Optimal, 90.0));
}

#[gpui::test]
fn justify_line_seams_round_trip(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| seams_round_trip(window, app, LineBreakMode::Justify, 90.0));
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

fn justify_cases() -> [(String, BlockKind, f32); 4] {
    [
        (CHINESE.to_owned(), BlockKind::Paragraph, 200.0),
        (format!("{CHINESE}{CHINESE}"), BlockKind::Paragraph, 260.0),
        (LATIN.to_owned(), BlockKind::Paragraph, 220.0),
        (REPEATED.repeat(2), BlockKind::Paragraph, 240.0),
    ]
}

#[gpui::test]
fn justify_mode_fills_the_measure(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let mut filled = 0usize;
        for (text, kind, width) in justify_cases() {
            let art = plan(window, app, LineBreakMode::Justify, &text, kind, f64::from(width));
            for (line_ix, line) in art.lines.iter().enumerate() {
                let rows = row_bounds(line).len();
                for row in 0..rows.saturating_sub(1) {
                    filled += 1;
                    let ink = row_ink(line, row);
                    assert!(
                        (ink - width).abs() < 0.5,
                        "{kind:?} {text:?} line {line_ix} row {row} ends at {ink}px, not at {width}px"
                    );
                }
            }
        }
        assert!(
            filled >= 6,
            "premise: the samples wrapped into several rows, got {filled}"
        );
    });
}

#[gpui::test]
fn justify_mode_leaves_the_ragged_rows_alone(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let mut checked = 0usize;
        for (text, kind, width) in justify_cases() {
            let art = plan(window, app, LineBreakMode::Justify, &text, kind, f64::from(width));
            for line in &art.lines {
                let rows = row_bounds(line);
                for row in rows.len().saturating_sub(1)..rows.len() {
                    checked += 1;
                    let ink = row_ink(line, row);
                    assert!(
                        ink < width - 1.0,
                        "{kind:?} {text:?} row {row} was stretched to {ink}px in a {width}px measure"
                    );
                }
            }
        }
        assert!(checked >= 4, "premise: {checked} rows were measured");
    });
}

#[gpui::test]
fn a_hard_break_ends_an_unjustified_row(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let text = format!("{LATIN}\n{LATIN}");
        let width = 90.0;
        let art = plan(
            window,
            app,
            LineBreakMode::Justify,
            &text,
            BlockKind::Paragraph,
            width,
        );
        assert_eq!(art.lines.len(), 2, "premise: two hard lines");
        for line in &art.lines {
            let rows = row_bounds(line);
            assert!(rows.len() >= 2, "premise: each hard line wrapped");
            let last = rows.len() - 1;
            assert!(
                row_ink(line, last) < width as f32 - 1.0,
                "the row in front of a newline was stretched to the measure"
            );
        }
    });
}

#[gpui::test]
fn justify_mode_widens_the_gaps_optimal_leaves_alone(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let mut moved = 0usize;
        for (text, kind, width) in justify_cases() {
            let optimal = plan(window, app, LineBreakMode::Optimal, &text, kind, f64::from(width));
            let justify = plan(window, app, LineBreakMode::Justify, &text, kind, f64::from(width));
            if positions(&optimal) != positions(&justify) {
                moved += 1;
            }
        }
        assert!(
            moved >= 3,
            "justify only moved the glyphs in {moved} of the samples, so the shifts are probably not wired up"
        );
    });
}

#[gpui::test]
fn headings_and_cells_are_never_justified(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        for (text, kind) in [
            (LATIN, BlockKind::Heading(1)),
            (LATIN, BlockKind::Heading(3)),
            (CHINESE, BlockKind::TableCell),
        ] {
            let optimal = plan(window, app, LineBreakMode::Optimal, text, kind, 60.0);
            let justify = plan(window, app, LineBreakMode::Justify, text, kind, 60.0);
            assert_eq!(
                positions(&optimal),
                positions(&justify),
                "{kind:?} should ignore the justify setting"
            );
            assert_eq!(seams(&optimal), seams(&justify));
        }
    });
}

#[gpui::test]
fn a_caret_on_a_justified_row_sits_where_the_text_was_drawn(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, app| {
        let width = 220.0;
        let shaper = shaper_for(window, app, LineBreakMode::Justify);
        let art = shaper.artifact(
            LATIN,
            &[],
            width,
            BlockKind::Paragraph,
            ShapeIdentity::default(),
        );
        assert!(art.rows >= 3, "premise: the paragraph wrapped");
        for line in &art.lines {
            let all = glyphs(line);
            let text: &str = line.text.as_ref();
            let rows = row_bounds(line);
            for (row, (start, end)) in rows.iter().copied().enumerate() {
                if row + 1 == rows.len() {
                    break;
                }
                let at = (start..end)
                    .rev()
                    .find(|&at| {
                        !text[all[at].0..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    })
                    .expect("a justified row ends on ink");
                let right = all
                    .get(at + 1)
                    .map_or(f32::from(line.unwrapped_layout.width), |&(_, x)| x);
                let advance = f64::from(right - all[at].1);
                let (x, seen) =
                    shaper.position_for_offset(&art, all[at].0, InlineAlign::Start, width);
                assert_eq!(seen as usize, row, "the offset maps back to its own row");
                assert!(
                    (x + advance - width).abs() < 0.5,
                    "row {row}: the caret on the last glyph sits at {x}px and the glyph is {advance}px wide, but the measure is {width}px"
                );
            }
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
