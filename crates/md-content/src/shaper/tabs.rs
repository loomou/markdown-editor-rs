use gpui::TextRun;
use std::rc::Rc;

pub(super) const TAB_COLS: usize = 4;

pub(super) fn has_tab(text: &str) -> bool {
    text.as_bytes().contains(&b'\t')
}

pub(super) fn expand_tabs(text: &str) -> String {
    let extra = text.bytes().filter(|&b| b == b'\t').count() * (TAB_COLS - 1);
    if extra == 0 {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + extra);
    for ch in text.chars() {
        if ch == '\t' {
            out.push_str("    ");
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn expand_text_runs(text: &str, mut runs: Vec<TextRun>) -> Vec<TextRun> {
    if !has_tab(text) {
        return runs;
    }
    let bytes = text.as_bytes();
    let mut at = 0usize;
    for run in &mut runs {
        let end = (at + run.len).min(bytes.len());
        let extra = bytes[at..end].iter().filter(|&&b| b == b'\t').count() * (TAB_COLS - 1);
        run.len += extra;
        at = end;
    }
    runs
}

pub(super) fn orig_to_exp(text: &str, off: usize) -> usize {
    let off = off.min(text.len());
    if !text.as_bytes()[..off].contains(&b'\t') {
        return off;
    }
    let mut exp = 0usize;
    let mut i = 0usize;
    while i < off {
        let ch = text[i..].chars().next().expect("char boundary");
        let n = ch.len_utf8();
        exp += if ch == '\t' { TAB_COLS } else { n };
        i += n;
    }
    exp
}

pub(super) fn exp_to_orig(text: &str, exp: usize) -> usize {
    if !has_tab(text) {
        return exp.min(text.len());
    }
    let mut o = 0usize;
    let mut e = 0usize;
    for ch in text.chars() {
        let n = ch.len_utf8();
        let w = if ch == '\t' { TAB_COLS } else { n };
        if e >= exp {
            return o;
        }
        if exp < e + w {
            if ch == '\t' && exp - e >= e + w - exp {
                return o + n;
            }
            return o;
        }
        e += w;
        o += n;
    }
    o
}

pub(super) fn prepare_shape(
    text: &str,
    runs: Vec<TextRun>,
) -> (String, Vec<TextRun>, Option<Rc<str>>) {
    if !has_tab(text) {
        return (text.to_string(), runs, None);
    }
    (
        expand_tabs(text),
        expand_text_runs(text, runs),
        Some(Rc::from(text)),
    )
}

pub(super) fn expand_line_colors<T: Copy>(
    text: &str,
    colors: Vec<Vec<(u32, T)>>,
) -> Vec<Vec<(u32, T)>> {
    if !has_tab(text) {
        return colors;
    }
    text.split('\n')
        .zip(colors)
        .map(|(line, runs)| {
            let bytes = line.as_bytes();
            let mut at = 0usize;
            runs.into_iter()
                .map(|(len, color)| {
                    let end = (at + len as usize).min(bytes.len());
                    let extra =
                        bytes[at..end].iter().filter(|&&b| b == b'\t').count() * (TAB_COLS - 1);
                    at = end;
                    (len + extra as u32, color)
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{exp_to_orig, expand_tabs, orig_to_exp};

    #[test]
    fn tab_expands_to_four_spaces() {
        assert_eq!(expand_tabs("\thello"), "    hello");
        assert_eq!(expand_tabs("a\tb\n\tc"), "a    b\n    c");
        assert_eq!(expand_tabs("hello"), "hello");
    }

    #[test]
    fn offsets_round_trip_at_tab_edges() {
        let s = "\thi";
        assert_eq!(orig_to_exp(s, 0), 0);
        assert_eq!(orig_to_exp(s, 1), 4);
        assert_eq!(orig_to_exp(s, 2), 5);
        assert_eq!(orig_to_exp(s, 3), 6);
        assert_eq!(exp_to_orig(s, 0), 0);
        assert_eq!(exp_to_orig(s, 1), 0);
        assert_eq!(exp_to_orig(s, 2), 1);
        assert_eq!(exp_to_orig(s, 4), 1);
        assert_eq!(exp_to_orig(s, 5), 2);
        assert_eq!(exp_to_orig(s, 6), 3);
    }

    #[test]
    fn cjk_offsets_skip_whole_chars() {
        let s = "你\t好";
        assert_eq!(orig_to_exp(s, 0), 0);
        assert_eq!(orig_to_exp(s, 3), 3);
        assert_eq!(orig_to_exp(s, 4), 7);
        assert_eq!(exp_to_orig(s, 3), 3);
        assert_eq!(exp_to_orig(s, 7), 4);
    }
}
