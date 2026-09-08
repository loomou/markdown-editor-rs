pub fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn prev_char_boundary(s: &str, i: usize) -> usize {
    if i > s.len() {
        return s.len();
    }
    let mut j = i.saturating_sub(1);
    while j > 0 && !s.is_char_boundary(j) {
        j -= 1;
    }
    j
}

pub fn next_char_boundary(s: &str, i: usize) -> usize {
    let i = i.min(s.len());
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

#[cfg(test)]
mod tests {
    use super::{floor_char_boundary, next_char_boundary, prev_char_boundary};

    #[test]
    fn character_boundaries_clamp_out_of_range_offsets() {
        assert_eq!(next_char_boundary("", usize::MAX), 0);
        assert_eq!(prev_char_boundary("", usize::MAX), 0);
        assert_eq!(floor_char_boundary("", usize::MAX), 0);
        assert_eq!(next_char_boundary("€", 1024), 3);
        assert_eq!(prev_char_boundary("€", 1024), 3);
        assert_eq!(
            next_char_boundary("a€", 1),
            4,
            "skips the whole multibyte char from after the a"
        );
        assert_eq!(prev_char_boundary("a€", 1), 0);
    }
}
