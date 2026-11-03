use super::{Item, Mode, Params, Placed, effective_width};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedLine {
    pub start: u32,
    pub end: u32,
    pub next: u32,
    pub ratio: f32,
    pub hyphen: bool,
    pub overfull: bool,
    pub forced: bool,
    pub item_start: u32,
    pub item_end: u32,
    pub break_item: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub lines: Vec<PlannedLine>,
    pub fallback: bool,
}

pub(crate) const TIGHT: u8 = 0;
pub(crate) const DECENT: u8 = 1;
pub(crate) const LOOSE: u8 = 2;
pub(crate) const VERY_LOOSE: u8 = 3;

const NO_NODE: u32 = u32::MAX;

pub(crate) fn fitness(ratio: f64) -> u8 {
    if ratio < -0.5 {
        TIGHT
    } else if ratio <= 0.5 {
        DECENT
    } else if ratio <= 1.0 {
        LOOSE
    } else {
        VERY_LOOSE
    }
}

pub(crate) fn badness(ratio: f64) -> f64 {
    let magnitude = ratio.abs();
    (100.0 * magnitude * magnitude * magnitude).min(10000.0)
}

const BALANCED_RATIO_CAP: f64 = 1.0e4;

pub(crate) fn balanced_badness(ratio: f64) -> f64 {
    let magnitude = ratio.abs().min(BALANCED_RATIO_CAP);
    100.0 * magnitude * magnitude * magnitude
}

pub(crate) fn ratio_of(natural: f64, stretch: f64, shrink: f64, width: f64) -> f64 {
    if natural < width {
        if stretch > 0.0 {
            (width - natural) / stretch
        } else {
            f64::INFINITY
        }
    } else if natural > width {
        if shrink > 0.0 {
            (width - natural) / shrink
        } else {
            f64::NEG_INFINITY
        }
    } else {
        0.0
    }
}

pub(crate) fn is_breakpoint(items: &[Placed], index: usize) -> bool {
    match items[index].item {
        Item::Glue { .. } => index > 0 && matches!(items[index - 1].item, Item::Box { .. }),
        Item::Penalty { cost, .. } => cost < f32::INFINITY,
        Item::Box { .. } => false,
    }
}

pub(crate) fn is_forced(item: Item) -> bool {
    matches!(item, Item::Penalty { cost, .. } if cost == f32::NEG_INFINITY)
}

pub(crate) struct LineCalc<'a> {
    text: &'a str,
    items: &'a [Placed],
    prefix: Vec<[f64; 3]>,
    emergency_prefix: Vec<u32>,
    mode: Mode,
    params: &'a Params,
    width: f64,
    em: f64,
    line_stretch: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Score {
    pub ratio: f64,
    pub badness: f64,
}

impl<'a> LineCalc<'a> {
    pub(crate) fn new(
        text: &'a str,
        items: &'a [Placed],
        width: f32,
        em: f32,
        mode: Mode,
        params: &'a Params,
    ) -> Self {
        let mut prefix = Vec::with_capacity(items.len() + 1);
        let mut emergency_prefix = Vec::with_capacity(items.len() + 1);
        let mut acc = [0.0f64; 3];
        let mut crossings = 0u32;
        prefix.push(acc);
        emergency_prefix.push(crossings);
        for placed in items {
            match placed.item {
                Item::Box { width } => acc[0] += f64::from(width),
                Item::Glue {
                    width,
                    stretch,
                    shrink,
                } => {
                    acc[0] += f64::from(width);
                    acc[1] += f64::from(stretch);
                    acc[2] += f64::from(shrink);
                }
                Item::Penalty { emergency, .. } => {
                    if emergency {
                        crossings += 1;
                    }
                }
            }
            prefix.push(acc);
            emergency_prefix.push(crossings);
        }
        let line_stretch = match mode {
            Mode::Justify => 0.0,
            Mode::Ragged | Mode::Balanced | Mode::Fill => f64::from(params.ragged_stretch_em * em),
        };
        Self {
            text,
            items,
            prefix,
            emergency_prefix,
            mode,
            params,
            width: effective_width(width),
            em: f64::from(em),
            line_stretch,
        }
    }

    pub(crate) fn breakpoints(&self) -> Vec<u32> {
        (0..self.items.len() as u32)
            .filter(|&i| is_breakpoint(self.items, i as usize))
            .collect()
    }

    pub(crate) fn item(&self, index: u32) -> Item {
        self.items[index as usize].item
    }

    pub(crate) fn byte_at(&self, index: u32) -> u32 {
        self.items
            .get(index as usize)
            .map_or(self.text.len() as u32, |p| p.byte)
    }

    pub(crate) fn content_end(&self, break_index: u32, final_break: bool) -> u32 {
        if final_break {
            return break_index;
        }
        let mut i = break_index as usize;
        while i > 0 && matches!(self.items[i - 1].item, Item::Glue { .. }) {
            i -= 1;
        }
        i as u32
    }

    pub(crate) fn next_item(&self, break_index: u32) -> u32 {
        let mut i = break_index as usize + 1;
        while i < self.items.len() {
            if matches!(self.items[i].item, Item::Box { .. }) {
                return i as u32;
            }
            i += 1;
        }
        self.items.len() as u32
    }

    pub(crate) fn natural_at(
        &self,
        start_item: u32,
        break_index: u32,
        final_break: bool,
    ) -> [f64; 3] {
        let end = self.content_end(break_index, final_break) as usize;
        let start = (start_item as usize).min(end);
        let penalty = match self.items[break_index as usize].item {
            Item::Penalty { width, .. } => f64::from(width),
            _ => 0.0,
        };
        [
            self.prefix[end][0] - self.prefix[start][0] + penalty,
            self.prefix[end][1] - self.prefix[start][1] + self.line_stretch,
            self.prefix[end][2] - self.prefix[start][2],
        ]
    }

    pub(crate) fn ratio_at(&self, start_item: u32, break_index: u32, final_break: bool) -> f64 {
        self.score_at(start_item, break_index, final_break).ratio
    }

    pub(crate) fn score_at(&self, start_item: u32, break_index: u32, final_break: bool) -> Score {
        let [natural, stretch, shrink] = self.natural_at(start_item, break_index, final_break);
        if natural < self.width
            && stretch <= 0.0
            && self.contains_emergency(start_item, break_index)
        {
            let unit = if self.em > 0.0 { self.em } else { self.width };
            let ratio = (self.width - natural) / unit;
            return Score {
                ratio,
                badness: 100.0 * ratio * ratio * ratio,
            };
        }
        let ratio = ratio_of(natural, stretch, shrink, self.width);
        Score {
            ratio,
            badness: self.badness_for(ratio),
        }
    }

    fn badness_for(&self, ratio: f64) -> f64 {
        match self.mode {
            Mode::Balanced => balanced_badness(ratio),
            Mode::Ragged | Mode::Justify | Mode::Fill => badness(ratio),
        }
    }

    fn contains_emergency(&self, start_item: u32, break_index: u32) -> bool {
        let from = (start_item as usize).min(self.emergency_prefix.len());
        let to = (break_index as usize + 1).min(self.emergency_prefix.len());
        to > from && self.emergency_prefix[to] > self.emergency_prefix[from]
    }

    pub(crate) fn emergency_break(&self, break_index: u32) -> bool {
        matches!(
            self.items
                .get(break_index as usize)
                .map(|placed| placed.item),
            Some(Item::Penalty {
                emergency: true,
                ..
            })
        )
    }

    pub(crate) fn runt_at(&self, start_byte: u32, end_byte: u32) -> bool {
        let limit = self.params.runt_max_chars as usize;
        if limit == 0 {
            return false;
        }
        let from = start_byte as usize;
        let to = (end_byte as usize).max(from);
        self.text
            .get(from..to)
            .is_none_or(|slice| slice.graphemes(true).take(limit).count() < limit)
    }

    pub(crate) fn demerits_at(
        &self,
        prev_fit: u8,
        prev_hyphen: bool,
        score: Score,
        break_index: u32,
        runt: bool,
    ) -> f64 {
        let (cost, flagged) = match self.items[break_index as usize].item {
            Item::Penalty { cost, flagged, .. } => (f64::from(cost), flagged),
            _ => (0.0, false),
        };
        let params = self.params;
        let mut demerits = (f64::from(params.line_penalty) + score.badness).powi(2);
        demerits += if cost >= 0.0 {
            cost * cost
        } else if cost > f64::NEG_INFINITY {
            -(cost * cost)
        } else {
            0.0
        };
        if prev_hyphen && flagged {
            demerits += f64::from(params.flagged_demerits);
        }
        if self.mode == Mode::Justify && prev_fit.abs_diff(fitness(score.ratio)) > 1 {
            demerits += f64::from(params.fitness_demerits);
        }
        if runt {
            demerits += f64::from(params.runt_demerits);
        }
        demerits
    }

    fn line_of(
        &self,
        start_item: u32,
        break_item: u32,
        last_break: u32,
        ratio: f64,
        overfull: bool,
    ) -> PlannedLine {
        let end_item = self.content_end(break_item, break_item == last_break);
        let item = self.item(break_item);
        PlannedLine {
            start: self.byte_at(start_item),
            end: self.byte_at(end_item),
            next: self.byte_at(self.next_item(break_item)),
            ratio: ratio as f32,
            hyphen: matches!(item, Item::Penalty { flagged: true, .. }),
            overfull,
            forced: is_forced(item),
            item_start: start_item,
            item_end: end_item,
            break_item,
        }
    }
}

#[derive(Clone, Copy)]
struct Node {
    pos: u32,
    prev: u32,
    fit: u8,
    total: f64,
    line: PlannedLine,
}

#[derive(Clone, Copy)]
struct Candidate {
    total: f64,
    prev: u32,
    ratio: f64,
}

fn beats(candidate: Candidate, current: Candidate) -> bool {
    candidate.total < current.total
        || (candidate.total == current.total && candidate.ratio < current.ratio)
}

fn beats_overfull(candidate: Candidate, current: Candidate) -> bool {
    candidate.total < current.total
        || (candidate.total == current.total && candidate.ratio > current.ratio)
}

pub fn break_lines(
    text: &str,
    items: &[Placed],
    width: f32,
    em: f32,
    mode: Mode,
    params: &Params,
) -> Plan {
    let calc = LineCalc::new(text, items, width, em, mode, params);
    if items.is_empty() {
        return Plan {
            lines: Vec::new(),
            fallback: true,
        };
    }
    let breaks = calc.breakpoints();
    let Some(&last_break) = breaks.last() else {
        return Plan {
            lines: vec![calc.line_of(0, 0, 0, 0.0, true)],
            fallback: true,
        };
    };
    if mode == Mode::Fill {
        return greedy(&calc, &breaks, last_break, false);
    }
    if items.len() > params.max_items as usize {
        crate::hot_path::add_kp_max_items();
        return greedy(&calc, &breaks, last_break, true);
    }

    let blank = PlannedLine {
        start: 0,
        end: 0,
        next: 0,
        ratio: 0.0,
        hyphen: false,
        overfull: false,
        forced: false,
        item_start: 0,
        item_end: 0,
        break_item: 0,
    };
    let mut nodes: Vec<Node> = vec![Node {
        pos: 0,
        prev: NO_NODE,
        fit: DECENT,
        total: 0.0,
        line: blank,
    }];
    let mut active: Vec<u32> = vec![0];
    let mut final_node = 0u32;

    for &break_item in &breaks {
        let final_break = break_item == last_break;
        let end_item = calc.content_end(break_item, final_break);
        let mut best: [Option<Candidate>; 4] = [None; 4];
        let mut worst: Option<Candidate> = None;
        let mut alive: Vec<u32> = Vec::with_capacity(active.len());

        for &index in &active {
            #[cfg(test)]
            visits::bump();
            let node = nodes[index as usize];
            let score = calc.score_at(node.pos, break_item, final_break);
            let ratio = score.ratio;
            let runt = final_break
                && !calc.emergency_break(node.line.break_item)
                && calc.runt_at(calc.byte_at(node.pos), calc.byte_at(end_item));
            let total =
                node.total + calc.demerits_at(node.fit, node.line.hyphen, score, break_item, runt);
            let candidate = Candidate {
                total,
                prev: index,
                ratio,
            };
            if ratio >= -1.0 {
                let fit = fitness(ratio) as usize;
                if best[fit].is_none_or(|current| beats(candidate, current)) {
                    best[fit] = Some(candidate);
                }
                alive.push(index);
            } else if worst.is_none_or(|current| beats_overfull(candidate, current)) {
                worst = Some(candidate);
            }
        }

        let mut pushed: Vec<u32> = Vec::new();
        let mut chosen: Option<(Candidate, u32)> = None;
        for (fit, candidate) in best.into_iter().enumerate() {
            let Some(candidate) = candidate else {
                continue;
            };
            let index = push_node(
                &mut nodes, &calc, candidate, fit as u8, last_break, break_item, false,
            );
            pushed.push(index);
            if chosen.is_none_or(|(current, _)| beats(candidate, current)) {
                chosen = Some((candidate, index));
            }
        }
        if pushed.is_empty() {
            let candidate = worst.unwrap_or_else(|| {
                let last = nodes.len() as u32 - 1;
                Candidate {
                    total: nodes[last as usize].total,
                    prev: last,
                    ratio: f64::NEG_INFINITY,
                }
            });
            let index = push_node(
                &mut nodes, &calc, candidate, DECENT, last_break, break_item, true,
            );
            pushed.push(index);
            chosen = Some((candidate, index));
        }
        if is_forced(calc.item(break_item)) {
            active = pushed;
        } else {
            alive.extend(pushed);
            active = alive;
        }
        if final_break {
            final_node = chosen.map_or_else(|| nodes.len() as u32 - 1, |(_, index)| index);
        }
    }

    let mut chain = Vec::new();
    let mut cursor = final_node;
    while cursor != NO_NODE {
        chain.push(cursor);
        cursor = nodes[cursor as usize].prev;
    }
    chain.reverse();
    let lines = chain
        .iter()
        .skip(1)
        .map(|&index| nodes[index as usize].line)
        .collect();
    Plan {
        lines,
        fallback: false,
    }
}

fn push_node(
    nodes: &mut Vec<Node>,
    calc: &LineCalc<'_>,
    candidate: Candidate,
    fit: u8,
    last_break: u32,
    break_item: u32,
    overfull: bool,
) -> u32 {
    let start_item = nodes[candidate.prev as usize].pos;
    let line = calc.line_of(
        start_item,
        break_item,
        last_break,
        candidate.ratio,
        overfull,
    );
    nodes.push(Node {
        pos: calc.next_item(break_item),
        prev: candidate.prev,
        fit,
        total: candidate.total,
        line,
    });
    nodes.len() as u32 - 1
}

fn greedy(calc: &LineCalc<'_>, breaks: &[u32], last_break: u32, fallback: bool) -> Plan {
    let mut lines = Vec::new();
    let mut start = 0u32;
    let mut best: Option<u32> = None;
    let mut index = 0usize;
    while index < breaks.len() {
        let break_item = breaks[index];
        if break_item <= start {
            index += 1;
            continue;
        }
        let final_break = break_item == last_break;
        let ratio = calc.ratio_at(start, break_item, final_break);
        if ratio >= -1.0 && !final_break && !is_forced(calc.item(break_item)) {
            best = Some(break_item);
            index += 1;
            continue;
        }
        if ratio < -1.0
            && let Some(previous) = best.take()
            && previous > start
        {
            let previous_ratio = calc.ratio_at(start, previous, previous == last_break);
            lines.push(calc.line_of(start, previous, last_break, previous_ratio, false));
            start = calc.next_item(previous);
            continue;
        }
        lines.push(calc.line_of(start, break_item, last_break, ratio, ratio < -1.0));
        start = calc.next_item(break_item);
        best = None;
        index += 1;
    }
    if let Some(previous) = best
        && previous > start
    {
        let ratio = calc.ratio_at(start, previous, previous == last_break);
        lines.push(calc.line_of(start, previous, last_break, ratio, false));
    }
    Plan { lines, fallback }
}

#[cfg(test)]
pub(crate) fn is_breakpoint_at(items: &[Placed], byte: u32) -> bool {
    items
        .iter()
        .enumerate()
        .any(|(index, placed)| placed.byte == byte && is_breakpoint(items, index))
}

#[cfg(test)]
pub(crate) fn plan_demerits(
    text: &str,
    items: &[Placed],
    plan: &Plan,
    width: f32,
    em: f32,
    mode: Mode,
    params: &Params,
) -> f64 {
    let calc = LineCalc::new(text, items, width, em, mode, params);
    let last_break = plan.lines.last().map_or(0, |line| line.break_item);
    let mut total = 0.0;
    let mut fit = DECENT;
    let mut hyphen = false;
    let mut previous_break: Option<u32> = None;
    for line in &plan.lines {
        let final_break = line.break_item == last_break;
        let score = calc.score_at(line.item_start, line.break_item, final_break);
        let runt = final_break
            && !previous_break.is_some_and(|at| calc.emergency_break(at))
            && calc.runt_at(line.start, line.end);
        total += calc.demerits_at(fit, hyphen, score, line.break_item, runt);
        fit = fitness(score.ratio);
        hyphen = line.hyphen;
        previous_break = Some(line.break_item);
    }
    total
}

#[cfg(test)]
pub(crate) mod visits {
    use std::cell::Cell;

    thread_local! {
        static COUNT: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn bump() {
        COUNT.with(|count| count.set(count.get() + 1));
    }

    pub(crate) fn take() -> u64 {
        COUNT.with(|count| count.replace(0))
    }
}
