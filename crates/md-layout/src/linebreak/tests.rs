use super::kp::{DECENT, LineCalc, fitness, plan_demerits};
use super::*;

const EM: f32 = 10.0;

fn params() -> Params {
    Params::default()
}

fn mono_clusters(text: &str) -> Clusters {
    let mut bytes = Vec::new();
    let mut xs = Vec::new();
    let mut x = 0.0f32;
    for (i, c) in text.char_indices() {
        bytes.push(i as u32);
        xs.push(x);
        x += if c.len_utf8() > 3 { 12.0 } else { 6.0 };
    }
    bytes.push(text.len() as u32);
    xs.push(x);
    Clusters::from_pairs(bytes.into_iter().zip(xs)).expect("monotone bytes")
}

fn no_ranges() -> [std::ops::Range<u32>; 0] {
    []
}

fn layout(text: &str, width: f32, mode: Mode, params: &Params) -> (Vec<Placed>, Plan) {
    let clusters = mono_clusters(text);
    let code_ranges = no_ranges();
    let opps = opportunities(text, &OppCtx { glue_before: &[] });
    let items = build_items(
        text,
        &clusters,
        &opps,
        &ItemCtx {
            mode,
            em: EM,
            width,
            params,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let plan = break_lines(text, &items, width, EM, mode, params);
    (items, plan)
}

fn ragged(text: &str, width: f32) -> (Vec<Placed>, Plan) {
    layout(text, width, Mode::Ragged, &params())
}

fn widths(
    text: &str,
    items: &[Placed],
    plan: &Plan,
    width: f32,
    mode: Mode,
    params: &Params,
) -> Vec<f32> {
    let calc = LineCalc::new(text, items, width, EM, mode, params);
    let last = plan.lines.len().saturating_sub(1);
    plan.lines
        .iter()
        .enumerate()
        .map(|(i, line)| calc.natural_at(line.item_start, line.break_item, i == last)[0] as f32)
        .collect()
}

fn break_bytes(text: &str, ctx: &OppCtx<'_>) -> Vec<u32> {
    opportunities(text, ctx)
        .into_iter()
        .map(|point| point.0)
        .collect()
}

fn brute_force(
    text: &str,
    items: &[Placed],
    width: f32,
    mode: Mode,
    params: &Params,
) -> Option<f64> {
    let calc = LineCalc::new(text, items, width, EM, mode, params);
    let breaks = calc.breakpoints();
    let count = breaks.len();
    if count == 0 || count > 16 {
        return None;
    }
    let last = breaks[count - 1];
    let mut best: Option<f64> = None;
    for mask in 0..(1u32 << (count - 1)) {
        let mut chosen: Vec<u32> = (0..count - 1)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| breaks[bit])
            .collect();
        chosen.push(last);
        let mut total = 0.0;
        let mut fit = DECENT;
        let mut hyphen = false;
        let mut start = 0u32;
        let mut feasible = true;
        for &break_item in &chosen {
            if start > break_item {
                feasible = false;
                break;
            }
            let final_break = break_item == last;
            let ratio = calc.ratio_at(start, break_item, final_break);
            if ratio.is_nan() || ratio < -1.0 {
                feasible = false;
                break;
            }
            let end_item = calc.content_end(break_item, final_break);
            let runt = final_break && calc.runt_at(calc.byte_at(start), calc.byte_at(end_item));
            total += calc.demerits_at(fit, hyphen, ratio, break_item, runt);
            fit = fitness(ratio);
            hyphen = matches!(calc.item(break_item), Item::Penalty { flagged: true, .. });
            start = calc.next_item(break_item);
        }
        if feasible {
            best = Some(best.map_or(total, |current: f64| current.min(total)));
        }
    }
    best
}

#[test]
fn plan_covers_the_paragraph_in_order() {
    let cases = [
        ("", 40.0),
        ("   ", 40.0),
        ("\n", 40.0),
        ("abc\ndef", 40.0),
        ("中文测试文本内容", 36.0),
        ("alpha beta gamma delta epsilon", 48.0),
        ("中 mix 文 abc 混合 内容", 60.0),
        (
            "https://example.com/a/very/long/path/that/keeps/going/forever",
            60.0,
        ),
    ];
    for (text, width) in cases {
        let (_, plan) = ragged(text, width);
        assert!(!plan.lines.is_empty(), "{text:?} produced no lines");
        assert_eq!(
            plan.lines.last().unwrap().next,
            text.len() as u32,
            "{text:?}"
        );
        let mut previous = None;
        for line in &plan.lines {
            assert!(line.start <= line.end, "{text:?}: {line:?}");
            assert!(line.item_start <= line.item_end, "{text:?}: {line:?}");
            if let Some(previous) = previous {
                assert!(line.next > previous, "{text:?} does not advance");
            }
            previous = Some(line.next);
            assert!(line.next <= text.len() as u32, "{text:?}: {line:?}");
        }
    }
}

#[test]
fn plan_is_deterministic() {
    let text = "中 mix 文 abc 混合 内容 with some latin words in between";
    for width in [30.0, 55.0, 80.0] {
        let (_, first) = ragged(text, width);
        let (_, second) = ragged(text, width);
        assert_eq!(first, second);
    }
}

#[test]
fn feasible_lines_are_never_marked_overfull() {
    let text = "alpha beta gamma delta epsilon zeta eta theta";
    let (_, plan) = ragged(text, 60.0);
    for line in &plan.lines {
        assert!(!line.overfull, "{line:?}");
    }
    let (_, cjk) = ragged("中文测试文本内容排版引擎", 42.0);
    for line in &cjk.lines {
        assert!(!line.overfull, "{line:?}");
    }
}

#[test]
fn newline_forces_a_line_break() {
    let (_, plan) = ragged("abc\ndef", 1000.0);
    assert_eq!(plan.lines.len(), 2);
    assert_eq!(plan.lines[0].start, 0);
    assert_eq!(plan.lines[0].end, 3);
    assert_eq!(plan.lines[0].next, 4);
    assert_eq!(plan.lines[1].start, 4);
    assert_eq!(plan.lines[1].next, 7);
}

#[test]
fn degenerate_widths_do_not_panic() {
    for width in [0.0, -5.0, f32::NAN, f32::INFINITY] {
        let (_, plan) = ragged("中文 abc def", width);
        assert!(!plan.lines.is_empty());
        assert_eq!(plan.lines.last().unwrap().next, "中文 abc def".len() as u32);
    }
    let (_, empty) = ragged("", 50.0);
    assert!(empty.lines.len() <= 1);
    let (_, spaces) = ragged("     ", 50.0);
    assert!(spaces.lines.len() <= 1);
}

#[test]
fn long_words_break_at_emergency_points() {
    let text = "https://example.com/a/very/long/path/that/keeps/going/forever";
    let (_, plan) = ragged(text, 60.0);
    assert!(plan.lines.len() > 1);
    for line in &plan.lines {
        assert!(!line.overfull, "{line:?}");
        assert!(line.end > line.start || plan.lines.len() == 1);
    }
}

#[test]
fn items_over_max_budget_fall_back_to_greedy() {
    let text = "alpha beta gamma delta epsilon zeta";
    let width = 60.0;
    let capped = Params {
        max_items: 4,
        ..params()
    };
    let (items, plan) = layout(text, width, Mode::Ragged, &capped);
    assert!(plan.fallback);
    assert_eq!(plan.lines.last().unwrap().next, text.len() as u32);

    let (_, optimal) = layout(text, width, Mode::Ragged, &params());
    assert!(!optimal.fallback);
    let greedy_cost = plan_demerits(text, &items, &plan, width, EM, Mode::Ragged, &capped);
    let optimal_cost = plan_demerits(text, &items, &optimal, width, EM, Mode::Ragged, &params());
    assert!(
        optimal_cost <= greedy_cost + 1e-6,
        "{optimal_cost} > {greedy_cost}"
    );
}

#[test]
fn optimal_matches_brute_force() {
    let params = params();
    let texts = [
        "中文测试文本内容",
        "alpha beta gamma delta",
        "a b c d e f",
        "中 mix 文 abc 混合内容",
    ];
    for text in texts {
        for width in [30.0, 42.0, 60.0, 96.0] {
            let (items, plan) = layout(text, width, Mode::Ragged, &params);
            let Some(best) = brute_force(text, &items, width, Mode::Ragged, &params) else {
                continue;
            };
            let got = plan_demerits(text, &items, &plan, width, EM, Mode::Ragged, &params);
            assert!(
                (got - best).abs() < 1e-6,
                "{text:?} @ {width}: {got} != {best}"
            );
        }
    }
}

#[test]
fn optimal_matches_brute_force_in_justify_mode() {
    let params = params();
    let texts = ["alpha beta gamma delta", "中 mix 文 abc 混合内容"];
    for text in texts {
        for width in [36.0, 54.0, 78.0] {
            let (items, plan) = layout(text, width, Mode::Justify, &params);
            let Some(best) = brute_force(text, &items, width, Mode::Justify, &params) else {
                continue;
            };
            let got = plan_demerits(text, &items, &plan, width, EM, Mode::Justify, &params);
            assert!(
                (got - best).abs() < 1e-6,
                "{text:?} @ {width}: {got} != {best}"
            );
        }
    }
}

#[test]
fn balanced_evens_out_line_lengths() {
    let text = "中".repeat(9);
    let width = 24.0;
    let params = params();
    let (ragged_items, ragged_plan) = layout(&text, width, Mode::Ragged, &params);
    let (balanced_items, balanced_plan) = layout(&text, width, Mode::Balanced, &params);
    let ragged_widths = widths(
        &text,
        &ragged_items,
        &ragged_plan,
        width,
        Mode::Ragged,
        &params,
    );
    let balanced_widths = widths(
        &text,
        &balanced_items,
        &balanced_plan,
        width,
        Mode::Balanced,
        &params,
    );
    assert!(balanced_widths.len() > 1);
    assert!(
        variance(&balanced_widths) < variance(&ragged_widths),
        "{balanced_widths:?} !< {ragged_widths:?}"
    );
}

fn variance(values: &[f32]) -> f32 {
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / values.len() as f32
}

#[test]
fn a_short_last_line_is_a_runt() {
    let text = "中中中";
    let width = 12.0;
    let (_, with_runt) = layout(text, width, Mode::Ragged, &params());
    let (_, without_runt) = layout(
        text,
        width,
        Mode::Ragged,
        &Params {
            runt_demerits: 0.0,
            ..params()
        },
    );
    assert_eq!(with_runt.lines.len(), 2);
    assert_eq!(without_runt.lines.len(), 2);
    assert_eq!(without_runt.lines[0].end, 6);
    assert_eq!(with_runt.lines[0].end, 3);
}

#[test]
fn justify_shifts_match_the_shortfall_of_each_line() {
    let text = "alpha beta gamma delta epsilon zeta";
    let width = 90.0;
    let params = params();
    let (items, plan) = layout(text, width, Mode::Justify, &params);
    let calc = LineCalc::new(text, &items, width, EM, Mode::Justify, &params);
    let shifts = glyph_shifts(&plan, &items);
    assert!(!shifts.is_empty());
    let mut cursor = 0usize;
    let mut accumulated = 0.0f32;
    let last_line = plan.lines.len() - 1;
    for (index, line) in plan.lines.iter().enumerate() {
        let mut current = accumulated;
        while cursor < shifts.len() && shifts[cursor].0 <= line.end {
            current = shifts[cursor].1;
            cursor += 1;
        }
        let moved = current - accumulated;
        accumulated = current;
        let [natural, _, _] = calc.natural_at(line.item_start, line.break_item, index == last_line);
        if index == last_line || line.overfull {
            assert_eq!(moved, 0.0);
        } else {
            let expected = width - natural as f32;
            assert!(
                (moved - expected).abs() < 1e-3,
                "line {index}: {moved} != {expected}"
            );
        }
    }
    assert_eq!(cursor, shifts.len());
}

#[test]
fn justify_glues_carry_stretch_and_shrink() {
    let clusters = mono_clusters("ab cd");
    let code_ranges = no_ranges();
    let opps = opportunities("ab cd", &OppCtx { glue_before: &[] });
    let justify = Params::default();
    let items = build_items(
        "ab cd",
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Justify,
            em: EM,
            width: 100.0,
            params: &justify,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let glue = items
        .iter()
        .find_map(|placed| match placed.item {
            Item::Glue {
                width,
                stretch,
                shrink,
            } if width > 0.0 => Some((width, stretch, shrink)),
            _ => None,
        })
        .expect("a word space");
    assert!((glue.1 - glue.0 * 0.5).abs() < 1e-4);
    assert!((glue.2 - glue.0 / 3.0).abs() < 1e-4);

    let items = build_items(
        "ab cd",
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Ragged,
            em: EM,
            width: 100.0,
            params: &justify,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let fill = items.len() - 2;
    assert!(items.iter().enumerate().all(|(index, placed)| index == fill
        || !matches!(
            placed.item,
            Item::Glue { stretch, shrink, .. } if stretch > 0.0 || shrink > 0.0
        )));
}

#[test]
fn item_encoding_follows_the_table() {
    let clusters = mono_clusters("ab cd");
    let code_ranges = no_ranges();
    let opps = opportunities("ab cd", &OppCtx { glue_before: &[] });
    let params = Params::default();
    let items = build_items(
        "ab cd",
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Ragged,
            em: EM,
            width: 100.0,
            params: &params,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let shape: Vec<(&str, u32)> = items
        .iter()
        .map(|placed| {
            let kind = match placed.item {
                Item::Box { .. } => "box",
                Item::Glue { .. } => "glue",
                Item::Penalty { .. } => "penalty",
            };
            (kind, placed.byte)
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            ("box", 0),
            ("glue", 2),
            ("box", 3),
            ("penalty", 5),
            ("glue", 5),
            ("penalty", 5)
        ]
    );
    assert!(matches!(items[3].item, Item::Penalty { cost, .. } if cost == f32::INFINITY));
    assert!(matches!(items[5].item, Item::Penalty { cost, .. } if cost == f32::NEG_INFINITY));
}

#[test]
fn nbsp_is_an_unbreakable_stretchy_glue() {
    let text = "ab\u{A0}cd";
    let clusters = mono_clusters(text);
    let code_ranges = no_ranges();
    let opps = opportunities(text, &OppCtx { glue_before: &[] });
    let params = Params::default();
    let items = build_items(
        text,
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Justify,
            em: EM,
            width: 100.0,
            params: &params,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let glue_at = items
        .iter()
        .position(|placed| matches!(placed.item, Item::Glue { width, .. } if width > 0.0))
        .expect("nbsp glue");
    assert!(matches!(items[glue_at - 1].item, Item::Penalty { cost, .. } if cost == f32::INFINITY));
    assert_eq!(
        break_bytes(text, &OppCtx { glue_before: &[] }),
        Vec::<u32>::new()
    );
}

#[test]
fn clusters_lookup() {
    let clusters = mono_clusters("abc");
    assert_eq!(clusters.bytes, vec![0, 1, 2, 3]);
    assert_eq!(clusters.width(0..3), 18.0);
    assert_eq!(clusters.width(1..2), 6.0);
    assert_eq!(clusters.x_at(2), 12.0);
    assert!(clusters.is_cluster_start(1));
    assert!(!clusters.is_cluster_start(4));
    assert!(Clusters::from_pairs([(0, 0.0), (0, 1.0)]).is_none());
    assert!(Clusters::from_pairs([(2, 0.0), (1, 1.0)]).is_none());
}

#[test]
fn closing_punctuation_never_starts_a_line() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "中文，中文。中文！中文）";
    let points = break_bytes(text, &ctx);
    for stop in ["，", "。", "！", "）"] {
        let at = text.find(stop).unwrap() as u32;
        assert!(!points.contains(&at), "{stop} may start a line");
    }
    assert!(!points.is_empty());
}

#[test]
fn opening_punctuation_never_ends_a_line() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "中文（中文「中文『中文";
    let points = break_bytes(text, &ctx);
    for start in ["（", "「", "『"] {
        let at = text.find(start).unwrap() as u32;
        assert!(
            !points.contains(&(at + start.len() as u32)),
            "{start} may end a line"
        );
    }
}

#[test]
fn repeated_marks_are_not_split() {
    let ctx = OppCtx { glue_before: &[] };
    for text in ["中文——中文", "中文……中文"] {
        let points = break_bytes(text, &ctx);
        for offset in 1..3 {
            let at = text.find('—').or_else(|| text.find('…')).unwrap() + offset;
            let at = at as u32;
            assert!(!points.contains(&at), "{text} splits at {at}");
        }
    }
}

#[test]
fn cjk_quotes_follow_op_and_cl_rules() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "中文“引号”中文";
    let points = break_bytes(text, &ctx);
    let open = text.find('“').unwrap() as u32;
    let close = text.find('”').unwrap() as u32;
    assert!(points.contains(&open), "may break before an opening quote");
    assert!(
        !points.contains(&(open + 3)),
        "must not break after an opening quote"
    );
    assert!(
        !points.contains(&close),
        "must not break before a closing quote"
    );
    assert!(
        points.contains(&(close + 3)),
        "may break after a closing quote"
    );
}

#[test]
fn latin_apostrophes_are_left_alone() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "it\u{2019}s a test";
    let points = break_bytes(text, &ctx);
    assert_eq!(points, vec![7, 9]);
}

#[test]
fn zwj_sequences_are_not_split() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "字\u{200D}字";
    let points = break_bytes(text, &ctx);
    assert!(!points.contains(&3), "{points:?}");
}

#[test]
fn leading_whitespace_never_breaks() {
    let ctx = OppCtx { glue_before: &[] };
    let text = "   中文测试";
    let points = break_bytes(text, &ctx);
    assert!(points.iter().all(|&point| point > 3), "{points:?}");
}

#[test]
fn glue_before_suppresses_a_break() {
    let text = "中 文";
    let plain = break_bytes(text, &OppCtx { glue_before: &[] });
    assert!(plain.contains(&4));
    let glued = break_bytes(text, &OppCtx { glue_before: &[4] });
    assert!(!glued.contains(&4));
}

#[test]
fn complex_context_is_detected() {
    assert!(has_complex_context("ไทย"));
    assert!(has_complex_context("abc ก"));

    assert!(!has_complex_context("中文测试"));
    assert!(!has_complex_context("plain latin text"));
}

#[test]
fn code_breaks_are_penalised() {
    let text = "abc/def";
    let clusters = mono_clusters(text);
    let range = 0u32..7u32;
    let code_ranges = [range];
    let opps = opportunities(text, &OppCtx { glue_before: &[] });
    assert!(opps.iter().any(|point| point.0 == 4));
    let params = Params::default();
    let items = build_items(
        text,
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Ragged,
            em: EM,
            width: 100.0,
            params: &params,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    let penalty = items
        .iter()
        .find(|placed| placed.byte == 4)
        .expect("break inside code");
    assert!(
        matches!(penalty.item, Item::Penalty { cost, .. } if cost == params.code_break_penalty)
    );
    let _ = clusters;
}

fn pseudo_random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

fn random_text(seed: &mut u64) -> String {
    let words = ["alpha", "beta", "中", "文", "ab", "γ", "x"];
    let count = 2 + pseudo_random(seed) % 6;
    let mut text = String::new();
    for index in 0..count {
        if index > 0 && !pseudo_random(seed).is_multiple_of(3) {
            text.push(' ');
        }
        text.push_str(words[(pseudo_random(seed) % words.len() as u64) as usize]);
    }
    text
}

#[test]
fn optimal_matches_brute_force_on_random_input() {
    let params = params();
    let mut seed = 0x5eed_1234u64;
    let mut checked = 0;
    for _ in 0..200 {
        let text = random_text(&mut seed);
        let width = 18.0 + (pseudo_random(&mut seed) % 60) as f32;
        for mode in [Mode::Ragged, Mode::Justify, Mode::Balanced] {
            let (items, plan) = layout(&text, width, mode, &params);
            let Some(best) = brute_force(&text, &items, width, mode, &params) else {
                continue;
            };
            let got = plan_demerits(&text, &items, &plan, width, EM, mode, &params);
            assert!(
                (got - best).abs() < 1e-6,
                "{text:?} @ {width} {mode:?}: {got} != {best}"
            );
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} cases were checked");
}

#[test]
fn optimal_is_never_worse_than_greedy() {
    let params = params();
    let mut seed = 0xd00d_f00du64;
    for _ in 0..60 {
        let text = random_text(&mut seed);
        let width = 24.0 + (pseudo_random(&mut seed) % 48) as f32;
        let (items, _) = layout(&text, width, Mode::Ragged, &params);
        let optimal = break_lines(&text, &items, width, EM, Mode::Ragged, &params);
        let capped = Params {
            max_items: 1,
            ..params
        };
        let greedy = break_lines(&text, &items, width, EM, Mode::Ragged, &capped);
        assert!(greedy.fallback);
        let optimal_cost = plan_demerits(&text, &items, &optimal, width, EM, Mode::Ragged, &params);
        let greedy_cost = plan_demerits(&text, &items, &greedy, width, EM, Mode::Ragged, &params);
        assert!(
            optimal_cost <= greedy_cost + 1e-6,
            "{text:?}: {optimal_cost} > {greedy_cost}"
        );
    }
}

#[test]
fn breakpoints_inside_a_glyph_cluster_are_dropped() {
    let text = "中 mix 文";
    let params = Params::default();
    let mut bytes: Vec<u32> = Vec::new();
    let mut xs: Vec<f32> = Vec::new();
    let mut x = 0.0f32;
    for (i, c) in text.char_indices() {
        let at = i as u32;
        if at != 8 {
            bytes.push(at);
            xs.push(x);
        }
        x += if c.len_utf8() > 3 { 12.0 } else { 6.0 };
    }
    bytes.push(text.len() as u32);
    xs.push(x);
    let clusters = Clusters::from_pairs(bytes.into_iter().zip(xs)).expect("monotone");
    let code_ranges = no_ranges();
    let opps = opportunities(text, &OppCtx { glue_before: &[] });
    assert!(opps.iter().any(|point| point.0 == 8));
    let items = build_items(
        text,
        &clusters,
        &opps,
        &ItemCtx {
            mode: Mode::Ragged,
            em: EM,
            width: 60.0,
            params: &params,
            code_ranges: &code_ranges,
            tab_ranges: &[],
        },
    );
    assert!(!crate::linebreak::kp::is_breakpoint_at(&items, 8));
}

#[test]
fn unbounded_width_keeps_the_paragraph_on_one_line() {
    let text = "alpha beta gamma delta 中文测试";
    let params = Params::default();
    for width in [10_000.0, f32::INFINITY] {
        let (_, plan) = layout(text, width, Mode::Ragged, &params);
        assert_eq!(plan.lines.len(), 1, "@ {width}");
        assert_eq!(plan.lines[0].next, text.len() as u32);
    }
    let empty = break_lines("", &[], 50.0, EM, Mode::Ragged, &params);
    assert!(empty.lines.is_empty());
}

#[test]
fn no_line_starts_or_ends_with_forbidden_punctuation() {
    let text = "在中文排版中，标点符号的处理非常重要。例如，逗号、句号、顿号等都不能出现在行首；\
        而左括号「（」和左引号「“」则不能出现在行尾。排版引擎必须遵守这些规则，否则版面会很难看。";
    let params = Params::default();
    for width in [30.0, 42.0, 60.0, 90.0] {
        for mode in [Mode::Ragged, Mode::Justify] {
            let (_, plan) = layout(text, width, mode, &params);
            assert!(plan.lines.len() > 1);
            for line in &plan.lines {
                let body = &text[line.start as usize..line.end as usize];
                if let Some(first) = body.chars().next() {
                    assert!(
                        !"\u{ff0c}\u{3002}\u{3001}\u{ff1b}\u{ff1a}\u{ff1f}\u{ff01}\u{ff09}\u{300d}\u{300f}\u{3011}\u{300b}".contains(first),
                        "@ {width} {mode:?}: {body:?} starts with {first}"
                    );
                }
                if let Some(last) = body.chars().next_back() {
                    assert!(
                        !"\u{ff08}\u{300c}\u{300e}\u{3010}\u{300a}\u{201c}".contains(last),
                        "@ {width} {mode:?}: {body:?} ends with {last}"
                    );
                }
            }
        }
    }
}
