use super::floor_char_boundary;
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn cluster_bounds(text: &str, i: usize) -> (usize, usize) {
    let prev = prev_grapheme_boundary(text, i);
    let hi = next_grapheme_boundary(text, prev);
    if hi > i {
        (prev, hi)
    } else {
        (i, next_grapheme_boundary(text, i))
    }
}

fn left_cluster(text: &str, i: usize) -> (usize, usize) {
    let prev = prev_grapheme_boundary(text, i);
    (prev, next_grapheme_boundary(text, prev))
}

fn cluster_base(text: &str, bounds: (usize, usize)) -> char {
    text[bounds.0..bounds.1]
        .chars()
        .next()
        .expect("cluster is non-empty")
}

pub fn next_word_boundary(text: &str, mut i: usize) -> usize {
    let len = text.len();
    i = floor_char_boundary(text, i.min(len));
    if i >= len {
        return len;
    }
    let bounds = cluster_bounds(text, i);
    if cluster_base(text, bounds).is_whitespace() {
        i = bounds.1;
        while i < len {
            let next = next_grapheme_boundary(text, i);
            if !cluster_base(text, (i, next)).is_whitespace() {
                break;
            }
            i = next;
        }
        return i;
    }
    let word = is_word_char(cluster_base(text, bounds));
    i = bounds.1;
    while i < len {
        let next = next_grapheme_boundary(text, i);
        let base = cluster_base(text, (i, next));
        if base.is_whitespace() || is_word_char(base) != word {
            break;
        }
        i = next;
    }
    while i < len {
        let next = next_grapheme_boundary(text, i);
        if !cluster_base(text, (i, next)).is_whitespace() {
            break;
        }
        i = next;
    }
    i
}

pub fn prev_word_boundary(text: &str, mut i: usize) -> usize {
    i = floor_char_boundary(text, i.min(text.len()));
    while i > 0 {
        let bounds = left_cluster(text, i);
        if !cluster_base(text, bounds).is_whitespace() {
            break;
        }
        i = bounds.0;
    }
    if i == 0 {
        return 0;
    }
    let bounds = left_cluster(text, i);
    let word = is_word_char(cluster_base(text, bounds));
    while i > 0 {
        let bounds = left_cluster(text, i);
        let base = cluster_base(text, bounds);
        if base.is_whitespace() || is_word_char(base) != word {
            break;
        }
        i = bounds.0;
    }
    i
}

pub fn prev_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
}

pub fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

pub fn word_span(text: &str, offset: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let mut bounds = text.split_word_bound_indices().peekable();
    while let Some((start, seg)) = bounds.next() {
        let end = start + seg.len();
        if offset > end {
            continue;
        }
        if start < offset && offset < end {
            return Some((start, end));
        }
        if offset <= start {
            return Some((start, end));
        }
        let word = is_word_seg(seg);
        let next_is_word = bounds.peek().map(|(_, next)| is_word_seg(next));
        if word && next_is_word != Some(true) {
            return Some((start, end));
        }
        if let Some(&(next_start, next)) = bounds.peek() {
            return Some((next_start, next_start + next.len()));
        }
        return Some((start, end));
    }
    None
}

fn is_word_seg(seg: &str) -> bool {
    matches!(seg.unicode_words().next(), Some(w) if w.len() == seg.len())
}

#[cfg(test)]
mod tests {
    use super::{
        next_grapheme_boundary, next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
        word_span,
    };
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn word_motion_lands_on_word_starts() {
        let t = "hello world  foo";
        assert_eq!(
            next_word_boundary(t, 0),
            6,
            "skips hello and the space after it"
        );
        assert_eq!(next_word_boundary(t, 6), 13, "eats both spaces together");
        assert_eq!(next_word_boundary(t, 13), t.len());
        assert_eq!(
            next_word_boundary(t, t.len()),
            t.len(),
            "the end does not move"
        );

        assert_eq!(prev_word_boundary(t, t.len()), 13);
        assert_eq!(
            prev_word_boundary(t, 13),
            6,
            "backs over the whitespace first, then to the word start"
        );
        assert_eq!(prev_word_boundary(t, 6), 0);
        assert_eq!(prev_word_boundary(t, 0), 0, "the start does not move");
    }

    #[test]
    fn word_motion_treats_punctuation_as_its_own_run() {
        let t = "a,,b";
        assert_eq!(
            next_word_boundary(t, 0),
            1,
            "the letter run stops before the comma"
        );
        assert_eq!(next_word_boundary(t, 1), 3, "two commas count as one run");
        assert_eq!(prev_word_boundary(t, 3), 1);
        let cn = "你好 世界";
        assert_eq!(next_word_boundary(cn, 0), "你好 ".len());
    }

    #[test]
    fn word_motion_treats_combining_marks_as_part_of_the_base() {
        let decomposed = "e\u{301}x word";
        assert_eq!(next_word_boundary(decomposed, 0), "e\u{301}x ".len());
        assert_eq!(prev_word_boundary(decomposed, "e\u{301}x ".len()), 0);
        let precomposed = "éx word";
        assert_eq!(next_word_boundary(precomposed, 0), "éx ".len());
        assert_eq!(prev_word_boundary(precomposed, "éx ".len()), 0);
        assert_eq!(next_word_boundary(decomposed, 1), "e\u{301}x ".len());
        assert_eq!(
            prev_word_boundary(decomposed, "e\u{301}x ".len() + 1),
            "e\u{301}x ".len()
        );
        let fam = "👨‍👩‍👧 next";
        assert_eq!(next_word_boundary(fam, 0), "👨‍👩‍👧 ".len());
        assert_eq!(prev_word_boundary(fam, fam.len()), "👨‍👩‍👧 ".len());
        let indic = "क्ष word";
        assert_eq!(next_word_boundary(indic, 0), "क्ष ".len());
    }

    #[test]
    fn word_motion_stays_on_character_boundaries() {
        let t = "héllo wörld";
        for i in 0..=t.len() {
            let n = next_word_boundary(t, i);
            let p = prev_word_boundary(t, i);
            assert!(
                t.is_char_boundary(n),
                "next landed on {n} from {i}, slicing into a character"
            );
            assert!(
                t.is_char_boundary(p),
                "prev landed on {p} from {i}, slicing into a character"
            );
        }
    }

    #[test]
    fn grapheme_motion_keeps_combining_marks_whole() {
        let t = "ae\u{301}b";
        assert_eq!(t.chars().count(), 4);
        assert_eq!(prev_grapheme_boundary(t, t.len()), 4, "in front of b");
        assert_eq!(
            prev_grapheme_boundary(t, 4),
            1,
            "the whole é backs off together, leaving no lone accent"
        );
        assert_eq!(next_grapheme_boundary(t, 1), 4);

        let fam = "👨‍👩‍👧";
        assert!(fam.chars().count() > 3);
        assert_eq!(
            prev_grapheme_boundary(fam, fam.len()),
            0,
            "the whole cluster backs off together"
        );
        assert_eq!(next_grapheme_boundary(fam, 0), fam.len());
        assert_eq!(prev_grapheme_boundary("", 0), 0);
        assert_eq!(next_grapheme_boundary("", 0), 0);
    }

    #[test]
    fn grapheme_cursor_agrees_with_the_iterator_scan() {
        let atoms = [
            "a",
            "é",
            "e\u{301}",
            "👨‍👩‍👧",
            "🇺🇸",
            "ᄀ",
            "ᅡ",
            "\r\n",
            "क्",
            "\u{fe0f}",
            "1\u{fe0f}\u{20e3}",
            "🙂",
            "中",
        ];
        for x in atoms {
            for y in atoms {
                let s = format!("a{x}{y}b");
                let mut offsets: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
                offsets.push(s.len());
                for offset in offsets {
                    let old_prev = s
                        .grapheme_indices(true)
                        .rev()
                        .find_map(|(i, _)| (i < offset).then_some(i))
                        .unwrap_or(0);
                    let old_next = s
                        .grapheme_indices(true)
                        .find_map(|(i, _)| (i > offset).then_some(i))
                        .unwrap_or(s.len());
                    assert_eq!(
                        prev_grapheme_boundary(&s, offset),
                        old_prev,
                        "s={s:?} offset={offset}"
                    );
                    assert_eq!(
                        next_grapheme_boundary(&s, offset),
                        old_next,
                        "s={s:?} offset={offset}"
                    );
                }
            }
        }
    }

    #[test]
    fn ascii_word_sticky_at_end() {
        let t = "hello world";
        assert_eq!(word_span(t, 0), Some((0, 5)));
        assert_eq!(word_span(t, 1), Some((0, 5)));
        assert_eq!(word_span(t, 5), Some((0, 5)));
        assert_eq!(word_span(t, 6), Some((6, 11)));
        assert_eq!(word_span(t, 11), Some((6, 11)));
    }

    #[test]
    fn space_run_interior() {
        assert_eq!(word_span("a  b", 2), Some((1, 3)));
    }

    #[test]
    fn apostrophe_stays_one_word() {
        let t = "don't";
        assert_eq!(word_span(t, 0), Some((0, t.len())));
        assert_eq!(word_span(t, 3), Some((0, t.len())));
    }

    #[test]
    fn punctuation_between_words() {
        let t = "hello,world";
        assert_eq!(word_span(t, 0), Some((0, 5)));
        assert_eq!(word_span(t, 5), Some((0, 5)));
        assert_eq!(word_span(t, 6), Some((6, 11)));
        assert_eq!(word_span("(hi)", 0), Some((0, 1)));
    }

    #[test]
    fn cjk_ideograph_is_its_own_word() {
        let t = "你好世界";
        let ni = "你".len();
        let hao = "好".len();
        assert_eq!(word_span(t, 0), Some((0, ni)));
        assert_eq!(word_span(t, ni), Some((ni, ni + hao)));
    }

    #[test]
    fn empty_and_mid_utf8() {
        assert_eq!(word_span("", 0), None);
        let t = "héllo";
        let mid = 2;
        assert!(!t.is_char_boundary(mid));
        let (lo, hi) = word_span(t, mid).expect("span");
        assert!(t.is_char_boundary(lo) && t.is_char_boundary(hi));
        assert_eq!((lo, hi), (0, t.len()));
    }

    #[test]
    fn previous_word_from_after_its_first_char_stays_in_that_word() {
        assert_eq!(prev_word_boundary("one two", 5), 4);
        assert_eq!(prev_word_boundary("one two", 6), 4);
        assert_eq!(prev_word_boundary("one  two", 6), 5);
        assert_eq!(prev_word_boundary("one two", 4), 0);
    }
}
