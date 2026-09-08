use std::ops::Range;

fn chars_match(a: char, b: char) -> bool {
    a == b || (a.is_ascii() && b.is_ascii() && a.eq_ignore_ascii_case(&b))
}

pub(super) struct Needle {
    chars: Vec<char>,
    fail: Vec<usize>,
    prefix_bytes: Vec<usize>,
}

impl Needle {
    pub(super) fn new(needle: &str) -> Self {
        let chars: Vec<char> = needle.chars().collect();
        let mut fail = vec![0usize; chars.len() + 1];
        let mut k = 0;
        for i in 1..chars.len() {
            while k > 0 && !chars_match(chars[i], chars[k]) {
                k = fail[k];
            }
            if chars_match(chars[i], chars[k]) {
                k += 1;
            }
            fail[i + 1] = k;
        }
        let mut prefix_bytes = Vec::with_capacity(chars.len() + 1);
        prefix_bytes.push(0);
        for &c in &chars {
            prefix_bytes.push(prefix_bytes.last().unwrap() + c.len_utf8());
        }
        Needle {
            chars,
            fail,
            prefix_bytes,
        }
    }
}

pub(super) fn find_with(hay: &str, needle: &Needle, max: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let m = needle.chars.len();
    if m == 0 || max == 0 {
        return out;
    }
    let mut j = 0;
    let mut byte = 0;
    for c in hay.chars() {
        while j > 0 && !chars_match(c, needle.chars[j]) {
            j = needle.fail[j];
        }
        if chars_match(c, needle.chars[j]) {
            j += 1;
        }
        byte += c.len_utf8();
        if j == m {
            out.push(byte - needle.prefix_bytes[m]..byte);
            if out.len() == max {
                break;
            }
            j = 0;
        }
    }
    out
}

pub(crate) fn find_in_display(hay: &str, needle: &str) -> Vec<Range<usize>> {
    find_with(hay, &Needle::new(needle), usize::MAX)
}

#[cfg(test)]
fn find_naive(hay: &str, needle: &str) -> Vec<Range<usize>> {
    let chars: Vec<char> = needle.chars().collect();
    let mut out = Vec::new();
    if chars.is_empty() {
        return out;
    }
    let mut byte = 0;
    while byte <= hay.len() {
        let Some(rest) = hay.get(byte..) else { break };
        let mut used = 0usize;
        let mut ok = true;
        for &n in &chars {
            let Some(h) = rest[used..].chars().next() else {
                ok = false;
                break;
            };
            if !chars_match(h, n) {
                ok = false;
                break;
            }
            used += h.len_utf8();
        }
        if ok {
            out.push(byte..byte + used);
            byte += used;
        } else {
            match hay[byte..].chars().next() {
                Some(c) => byte += c.len_utf8(),
                None => break,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Needle, find_in_display, find_naive, find_with};

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

    #[test]
    fn find_is_non_overlapping() {
        assert_eq!(find_in_display("aaaa", "aa"), vec![0..2, 2..4]);
    }

    #[test]
    fn find_across_mixed_case_needle() {
        assert_eq!(find_in_display("aAAa", "aAa"), vec![0..3]);
        assert_eq!(find_in_display("AaA", "aA"), vec![0..2]);
    }

    #[test]
    fn find_respects_the_limit() {
        let needle = Needle::new("aa");
        assert_eq!(find_with("aaaa", &needle, 1), vec![0..2]);
        assert_eq!(find_with("aaaa", &needle, 3), vec![0..2, 2..4]);
        assert!(find_with("aaaa", &needle, 0).is_empty());
    }

    #[test]
    fn matches_naive_reference() {
        let alphabet = ['a', 'A', 'b', 'B', 'é', 'ü', '😀', ' '];
        let mut state = 0x2545F491u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..300 {
            let hay_len = (next() % 40) as usize;
            let needle_len = (next() % 6) as usize;
            let pick = |n: u64| alphabet[(n % alphabet.len() as u64) as usize];
            let hay: String = (0..hay_len).map(|_| pick(next())).collect();
            let needle: String = (0..needle_len).map(|_| pick(next())).collect();
            assert_eq!(
                find_in_display(&hay, &needle),
                find_naive(&hay, &needle),
                "hay={hay:?} needle={needle:?}"
            );
        }
    }
}
