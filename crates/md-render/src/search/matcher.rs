use md_core::doc::next_char_boundary;
use std::ops::Range;

fn chars_match(a: char, b: char) -> bool {
    a == b || (a.is_ascii() && b.is_ascii() && a.eq_ignore_ascii_case(&b))
}

fn match_from(hay: &str, start: usize, needle: &[char]) -> Option<usize> {
    let mut rest = hay.get(start..)?;
    let mut used = 0usize;
    for &n in needle {
        let mut cs = rest.chars();
        let h = cs.next()?;
        if !chars_match(h, n) {
            return None;
        }
        let adv = h.len_utf8();
        used += adv;
        rest = &rest[adv..];
    }
    Some(start + used)
}

fn next_match_start(hay: &str, from: usize, first: char) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    if first.is_ascii() {
        let lo = first.to_ascii_lowercase() as u8;
        let up = first.to_ascii_uppercase() as u8;
        hay.as_bytes()[from..]
            .iter()
            .position(|&b| b == lo || b == up)
            .map(|i| from + i)
    } else {
        let rest = hay.get(from..)?;
        let mut off = 0;
        for c in rest.chars() {
            if c == first {
                return Some(from + off);
            }
            off += c.len_utf8();
        }
        None
    }
}

pub(crate) fn find_in_display(hay: &str, needle: &str) -> Vec<Range<usize>> {
    find_in_display_limited(hay, needle, usize::MAX)
}

pub(super) fn find_in_display_limited(hay: &str, needle: &str, max: usize) -> Vec<Range<usize>> {
    if needle.is_empty() || max == 0 {
        return Vec::new();
    }
    let needle: Vec<char> = needle.chars().collect();
    let first = needle[0];
    let mut out = Vec::new();
    let mut byte = 0;
    while out.len() < max {
        let Some(start) = next_match_start(hay, byte, first) else {
            break;
        };
        if let Some(end) = match_from(hay, start, &needle) {
            out.push(start..end);
            byte = end;
        } else {
            byte = next_char_boundary(hay, start);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::find_in_display;

    #[test]
    fn find_is_ascii_case_insensitive() {
        assert_eq!(find_in_display("Hello", "ell"), vec![1..4]);
        assert_eq!(find_in_display("ABC", "b"), vec![1..2]);
        assert_eq!(find_in_display("aaa", "aa"), vec![0..2]);
    }

    #[test]
    fn find_keeps_non_ascii_exact() {
        assert_eq!(find_in_display("αβγδ", "βγ"), vec![2..6]);
        assert!(find_in_display("αβ", "a").is_empty());
    }

    #[test]
    fn find_returns_utf8_byte_ranges_at_character_boundaries() {
        assert_eq!(find_in_display("a😀A😀", "😀"), vec![1..5, 6..10]);
        assert!(find_in_display("abc", "abcd").is_empty());
    }

    #[test]
    fn find_empty_needle() {
        assert!(find_in_display("abc", "").is_empty());
    }
}
