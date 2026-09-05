use super::{floor_char_boundary, prev_char_boundary};
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn next_word_boundary(text: &str, mut i: usize) -> usize {
    let len = text.len();
    i = floor_char_boundary(text, i.min(len));
    if i >= len {
        return len;
    }
    let start = text[i..].chars().next().expect("i < len");
    if start.is_whitespace() {
        while let Some(c) = text[i..].chars().next() {
            if !c.is_whitespace() {
                break;
            }
            i += c.len_utf8();
        }
        return i;
    }

    let word = is_word_char(start);
    while let Some(c) = text[i..].chars().next() {
        if c.is_whitespace() || is_word_char(c) != word {
            break;
        }
        i += c.len_utf8();
    }

    while let Some(c) = text[i..].chars().next() {
        if !c.is_whitespace() {
            break;
        }
        i += c.len_utf8();
    }
    i
}

pub fn prev_word_boundary(text: &str, mut i: usize) -> usize {
    i = floor_char_boundary(text, i.min(text.len()));
    if i == 0 {
        return 0;
    }
    i = prev_char_boundary(text, i);
    while i > 0 {
        let c = text[..i].chars().next_back().expect("i > 0");
        if !c.is_whitespace() {
            break;
        }
        i = prev_char_boundary(text, i);
    }
    if i == 0 {
        return 0;
    }
    let c = text[..i].chars().next_back().expect("i > 0");
    if c.is_whitespace() {
        return i;
    }
    let word = is_word_char(c);
    while i > 0 {
        let Some(c) = text[..i].chars().next_back() else {
            break;
        };
        if c.is_whitespace() || is_word_char(c) != word {
            break;
        }
        i = prev_char_boundary(text, i);
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
        assert_eq!(
            next_word_boundary(t, 6),
            13,
            "the two spaces are eaten together"
        );
        assert_eq!(next_word_boundary(t, 13), t.len());
        assert_eq!(
            next_word_boundary(t, t.len()),
            t.len(),
            "does not move at the end"
        );

        assert_eq!(prev_word_boundary(t, t.len()), 13);
        assert_eq!(
            prev_word_boundary(t, 13),
            6,
            "steps back over the whitespace, then to the word start"
        );
        assert_eq!(prev_word_boundary(t, 6), 0);
        assert_eq!(prev_word_boundary(t, 0), 0, "does not move at the start");
    }

    #[test]
    fn word_motion_treats_punctuation_as_its_own_run() {
        let t = "a,,b";
        assert_eq!(
            next_word_boundary(t, 0),
            1,
            "the letter run ends before the comma"
        );
        assert_eq!(next_word_boundary(t, 1), 3, "the two commas form one run");
        assert_eq!(prev_word_boundary(t, 3), 1);

        let cn = "你好 世界";
        assert_eq!(next_word_boundary(cn, 0), "你好 ".len());
    }

    #[test]
    fn word_motion_stays_on_character_boundaries() {
        let t = "héllo wörld";
        for i in 0..=t.len() {
            let n = next_word_boundary(t, i);
            let p = prev_word_boundary(t, i);
            assert!(
                t.is_char_boundary(n),
                "next from {i} lands on {n}, splitting a character"
            );
            assert!(
                t.is_char_boundary(p),
                "prev from {i} lands on {p}, splitting a character"
            );
        }
    }

    #[test]
    fn grapheme_motion_keeps_combining_marks_whole() {
        let t = "ae\u{301}b";
        assert_eq!(t.chars().count(), 4);
        assert_eq!(prev_grapheme_boundary(t, t.len()), 4, "right before b");
        assert_eq!(
            prev_grapheme_boundary(t, 4),
            1,
            "the whole é steps back together, leaving no stray accent"
        );
        assert_eq!(next_grapheme_boundary(t, 1), 4);

        let fam = "👨‍👩‍👧";
        assert!(fam.chars().count() > 3);
        assert_eq!(
            prev_grapheme_boundary(fam, fam.len()),
            0,
            "the whole sequence steps back together"
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
}
