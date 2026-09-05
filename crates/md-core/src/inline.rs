use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineMarks {
    bits: u16,
}

impl InlineMarks {
    pub const NONE: Self = Self { bits: 0 };
    pub const EM: Self = Self { bits: 1 };
    pub const STRONG: Self = Self { bits: 2 };
    pub const STRIKE: Self = Self { bits: 4 };
    pub const CODE: Self = Self { bits: 8 };
    pub const FOOTNOTE: Self = Self { bits: 16 };
    pub(crate) const IMAGE: Self = Self { bits: 32 };
    pub const SUPER: Self = Self { bits: 64 };
    pub const SUB: Self = Self { bits: 128 };
    pub(crate) const MATH_INLINE: Self = Self { bits: 256 };
    pub const MATH_DISPLAY: Self = Self { bits: 512 };
    pub const SYNTAX: Self = Self { bits: 1024 };
    pub(crate) const ATOMIC: Self = Self {
        bits: Self::IMAGE.bits | Self::MATH_INLINE.bits | Self::MATH_DISPLAY.bits,
    };

    pub fn is_math(self) -> bool {
        self.contains(Self::MATH_INLINE) || self.contains(Self::MATH_DISPLAY)
    }

    pub fn is_image(self) -> bool {
        self.contains(Self::IMAGE)
    }

    pub fn is_script(self) -> bool {
        self.contains(Self::SUPER) || self.contains(Self::SUB)
    }

    pub fn is_atomic(self) -> bool {
        self.is_math() || self.is_image()
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub(crate) fn without(self, flag: Self) -> Self {
        Self {
            bits: self.bits & !flag.bits,
        }
    }

    pub fn contains(self, flag: Self) -> bool {
        self.bits & flag.bits == flag.bits
    }

    pub fn is_syntax(self) -> bool {
        self.contains(Self::SYNTAX)
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineRun {
    pub display_range: Range<u32>,
    pub source_range: Option<Range<u32>>,
    pub marks: InlineMarks,
    pub link: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InlineAlign {
    #[default]
    Start,
    Center,
    End,
}

pub fn covering_runs(text_len: u32, runs: &[InlineRun]) -> Vec<InlineRun> {
    if text_len == 0 {
        return Vec::new();
    }
    if runs.is_empty() {
        return vec![InlineRun {
            display_range: 0..text_len,
            source_range: None,
            marks: InlineMarks::NONE,
            link: None,
        }];
    }
    let mut out = Vec::new();
    let mut at = 0u32;
    for r in runs {
        let start = r.display_range.start.clamp(at, text_len);
        let end = r.display_range.end.min(text_len).max(start);
        if start > at {
            out.push(InlineRun {
                display_range: at..start,
                source_range: None,
                marks: InlineMarks::NONE,
                link: None,
            });

            at = start;
        }
        if end > start {
            out.push(InlineRun {
                display_range: start..end,
                source_range: r.source_range.clone(),
                marks: r.marks,
                link: r.link,
            });
            at = end;
        }
    }
    if at < text_len {
        out.push(InlineRun {
            display_range: at..text_len,
            source_range: None,
            marks: InlineMarks::NONE,
            link: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{InlineMarks, InlineRun, covering_runs};

    fn run(display: std::ops::Range<u32>, marks: InlineMarks) -> InlineRun {
        InlineRun {
            display_range: display,
            source_range: None,
            marks,
            link: None,
        }
    }

    fn shape(runs: &[InlineRun]) -> Vec<(u32, u32, InlineMarks)> {
        runs.iter()
            .map(|r| (r.display_range.start, r.display_range.end, r.marks))
            .collect()
    }

    #[test]
    fn covering_runs_fills_gaps_before_between_and_after() {
        let got = covering_runs(
            10,
            &[run(2..4, InlineMarks::EM), run(6..7, InlineMarks::STRONG)],
        );
        assert_eq!(
            shape(&got),
            vec![
                (0, 2, InlineMarks::NONE),
                (2, 4, InlineMarks::EM),
                (4, 6, InlineMarks::NONE),
                (6, 7, InlineMarks::STRONG),
                (7, 10, InlineMarks::NONE),
            ]
        );
    }
    #[test]
    fn covering_runs_of_empty_text_is_empty() {
        assert!(covering_runs(0, &[]).is_empty());
        assert!(covering_runs(0, &[run(0..3, InlineMarks::EM)]).is_empty());
    }

    #[test]
    fn covering_runs_without_runs_covers_everything_unmarked() {
        assert_eq!(
            shape(&covering_runs(5, &[])),
            vec![(0, 5, InlineMarks::NONE)]
        );
    }

    #[test]
    fn covering_runs_clamps_a_run_reaching_past_the_end() {
        let got = covering_runs(4, &[run(2..9, InlineMarks::EM)]);
        assert_eq!(
            shape(&got),
            vec![(0, 2, InlineMarks::NONE), (2, 4, InlineMarks::EM)]
        );
    }

    #[test]
    fn covering_runs_output_is_contiguous_and_exact() {
        let got = covering_runs(
            12,
            &[
                run(0..3, InlineMarks::CODE),
                run(5..7, InlineMarks::EM),
                run(9..12, InlineMarks::STRIKE),
            ],
        );
        assert_eq!(got.first().expect("non-empty").display_range.start, 0);
        assert_eq!(got.last().expect("non-empty").display_range.end, 12);
        for pair in got.windows(2) {
            assert_eq!(pair[0].display_range.end, pair[1].display_range.start);
            assert!(pair[0].display_range.start < pair[0].display_range.end);
        }
    }

    #[test]
    fn covering_runs_steps_over_a_zero_width_run() {
        let got = covering_runs(
            12,
            &[
                run(0..3, InlineMarks::CODE),
                run(5..5, InlineMarks::EM),
                run(9..12, InlineMarks::STRIKE),
            ],
        );
        assert_eq!(
            shape(&got),
            vec![
                (0, 3, InlineMarks::CODE),
                (3, 5, InlineMarks::NONE),
                (5, 9, InlineMarks::NONE),
                (9, 12, InlineMarks::STRIKE),
            ]
        );
    }

    #[test]
    fn covering_runs_of_a_run_past_the_end_covers_once() {
        assert_eq!(
            shape(&covering_runs(4, &[run(6..8, InlineMarks::EM)])),
            vec![(0, 4, InlineMarks::NONE)]
        );
    }

    #[test]
    fn covering_runs_partitions_the_text_whatever_comes_in() {
        let cases: [&[InlineRun]; 5] = [
            &[run(5..8, InlineMarks::EM), run(2..4, InlineMarks::CODE)],
            &[run(0..5, InlineMarks::EM), run(3..8, InlineMarks::CODE)],
            &[run(3..3, InlineMarks::EM), run(3..3, InlineMarks::CODE)],
            &[run(9..9, InlineMarks::EM)],
            &[run(0..0, InlineMarks::EM), run(4..6, InlineMarks::CODE)],
        ];
        for runs in cases {
            let got = covering_runs(10, runs);
            assert_eq!(
                got.first().expect("non-empty").display_range.start,
                0,
                "{runs:?} did not hug the start"
            );
            assert_eq!(
                got.last().expect("non-empty").display_range.end,
                10,
                "{runs:?} did not reach the end"
            );
            for r in &got {
                assert!(
                    r.display_range.start < r.display_range.end,
                    "{runs:?} produced a zero-width run {:?}",
                    r.display_range
                );
            }
            for pair in got.windows(2) {
                assert_eq!(
                    pair[0].display_range.end, pair[1].display_range.start,
                    "{runs:?} left a gap or an overlap"
                );
            }
        }
    }

    #[test]
    fn covering_runs_keeps_source_and_link_only_on_covered_runs() {
        let mut marked = run(1..3, InlineMarks::IMAGE);
        marked.source_range = Some(7..20);
        marked.link = Some(42);

        let got = covering_runs(4, &[marked]);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].source_range, None);
        assert_eq!(got[0].link, None);
        assert_eq!(got[1].source_range, Some(7..20));
        assert_eq!(got[1].link, Some(42));
        assert_eq!(got[2].source_range, None);
        assert_eq!(got[2].link, None);
    }

    #[test]
    fn union_and_without_are_inverses() {
        let both = InlineMarks::EM.union(InlineMarks::CODE);
        assert!(both.contains(InlineMarks::EM));
        assert!(both.contains(InlineMarks::CODE));
        assert!(!both.contains(InlineMarks::STRONG));

        assert_eq!(both.without(InlineMarks::CODE), InlineMarks::EM);
        assert!(both.without(InlineMarks::EM).contains(InlineMarks::CODE));
        assert!(
            both.without(InlineMarks::EM)
                .without(InlineMarks::CODE)
                .is_empty()
        );
        assert!(InlineMarks::NONE.is_empty());
        assert!(!both.is_empty());
    }

    #[test]
    fn atomic_covers_image_and_math_but_not_styling() {
        assert!(InlineMarks::IMAGE.is_atomic());
        assert!(InlineMarks::IMAGE.is_image());
        assert!(InlineMarks::MATH_INLINE.is_atomic());
        assert!(InlineMarks::MATH_DISPLAY.is_atomic());
        assert!(InlineMarks::MATH_INLINE.is_math());

        assert!(!InlineMarks::EM.is_atomic());
        assert!(!InlineMarks::SYNTAX.is_atomic());
        assert!(InlineMarks::SYNTAX.is_syntax());
        assert!(!InlineMarks::EM.is_syntax());

        assert!(InlineMarks::ATOMIC.contains(InlineMarks::IMAGE));
        assert!(InlineMarks::ATOMIC.contains(InlineMarks::MATH_INLINE));
        assert!(InlineMarks::ATOMIC.contains(InlineMarks::MATH_DISPLAY));
        assert!(!InlineMarks::ATOMIC.contains(InlineMarks::EM));
    }

    #[test]
    fn script_covers_super_and_sub_only() {
        assert!(InlineMarks::SUPER.is_script());
        assert!(InlineMarks::SUB.is_script());
        assert!(!InlineMarks::EM.is_script());
        assert!(!InlineMarks::NONE.is_script());
    }
}
