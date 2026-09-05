pub(crate) fn is_atx_commit(source: &str) -> bool {
    let s = normalize_source(source);
    let hashes = s.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return false;
    }
    s.as_bytes().get(hashes) == Some(&b' ')
}

pub(crate) fn is_quote_commit(source: &str) -> bool {
    let s = normalize_source(source);
    if s.contains('\n') {
        return false;
    }
    let indent = s.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return false;
    }
    let rest = s.get(indent..).unwrap_or("");
    rest.starts_with('>') && !rest.starts_with(">>") && rest.len() > 1
}

pub(crate) fn is_thematic_break_line(source: &str) -> bool {
    let s = normalize_source(source);
    if s.is_empty() || s.contains('\n') {
        return false;
    }
    let mut marker = None;
    let mut n = 0usize;
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            continue;
        }
        if !matches!(c, '-' | '*' | '_') {
            return false;
        }
        match marker {
            None => marker = Some(c),
            Some(m) if m != c => return false,
            Some(_) => {}
        }
        n += 1;
    }
    n >= 3
}

pub(crate) fn is_open_fence_line(source: &str) -> bool {
    let s = normalize_source(source);
    if s.is_empty() || s.contains('\n') {
        return false;
    }
    fence_marker(s).is_some()
}

pub(crate) fn is_math_fence_line(source: &str) -> bool {
    let s = normalize_source(source);
    if s.contains('\n') {
        return false;
    }
    let indent = s.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return false;
    }
    s.get(indent..) == Some("$$")
}

pub(crate) fn close_fence_delete_range(s: &str, offset: usize) -> Option<std::ops::Range<usize>> {
    let (start, end) = line_range(s, offset);
    if !is_close_fence_line(s.get(start..end).unwrap_or("")) {
        return None;
    }
    let mut from = start;
    let mut to = end;
    if from > 0 && s.as_bytes()[from - 1] == b'\n' {
        from -= 1;
    } else if to < s.len() && s.as_bytes()[to] == b'\n' {
        to += 1;
    }
    Some(from..to)
}

pub(crate) fn line_range(s: &str, offset: usize) -> (usize, usize) {
    let offset = super::floor_char_boundary(s, offset.min(s.len()));
    let start = s[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = s[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(s.len());
    (start, end)
}

fn is_close_fence_line(line: &str) -> bool {
    let t = line.trim_end_matches('\r').trim_end();
    let Some((_, end)) = fence_marker(t) else {
        return false;
    };
    t.get(end..)
        .is_some_and(|tail| tail.bytes().all(|b| b == b' '))
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 3 && bytes[i] == b' ' {
        i += 1;
    }
    let rest = line.get(i..)?;
    let ch = if rest.starts_with("```") {
        '`'
    } else if rest.starts_with("~~~") {
        '~'
    } else {
        return None;
    };
    let n = rest.chars().take_while(|c| *c == ch).count();
    if n < 3 {
        return None;
    }
    Some((ch, i + n))
}

pub(crate) fn normalize_source(s: &str) -> &str {
    s.trim_end_matches(['\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::super::bind::{quote_lead_paragraph, unique_root};
    use super::super::{editor_options, load_markdown};
    use super::{
        close_fence_delete_range, is_atx_commit, is_math_fence_line, is_open_fence_line,
        is_quote_commit, is_thematic_break_line, line_range,
    };
    use crate::block::BlockKind;

    #[test]
    fn atx_commit_needs_space_after_hashes() {
        assert!(!is_atx_commit("#"));
        assert!(!is_atx_commit("###"));
        assert!(!is_atx_commit("#title"));
        assert!(!is_atx_commit("####### title"));
        assert!(is_atx_commit("# "));
        assert!(is_atx_commit("### title"));
        assert!(!is_atx_commit("hello # "));
    }

    #[test]
    fn quote_commit_needs_marker_plus_more() {
        assert!(!is_quote_commit(">"));
        assert!(is_quote_commit("> "));
        assert!(is_quote_commit(">hi"));
        assert!(is_quote_commit("> hi"));
        assert!(!is_quote_commit(">> hi"));
        assert!(!is_quote_commit("hello > "));
        let empty = load_markdown("> ", editor_options());
        assert!(quote_lead_paragraph(&empty).is_some());
        let nested = load_markdown("> > hi", editor_options());
        assert!(quote_lead_paragraph(&nested).is_none());
    }

    #[test]
    fn fence_line_helpers() {
        assert!(is_open_fence_line("```rust"));
        assert!(is_open_fence_line("~~~"));
        assert!(!is_open_fence_line("`a`"));
        assert!(!is_open_fence_line("hello"));

        assert!(!is_open_fence_line("$$"));
        assert!(is_math_fence_line("$$"));
        assert!(is_math_fence_line("   $$"));
        assert!(!is_math_fence_line("    $$"));
        assert!(!is_math_fence_line("$"));
        assert!(!is_math_fence_line("$$$"));
        assert!(!is_math_fence_line("$$x"));
        assert!(!is_math_fence_line("x$$"));
        assert!(!is_math_fence_line("$$\n$$"));
        assert!(is_thematic_break_line("---"));
        assert!(is_thematic_break_line("***"));
        assert!(is_thematic_break_line("___"));
        assert!(is_thematic_break_line("- - -"));
        assert!(!is_thematic_break_line("--"));
        assert!(!is_thematic_break_line("```"));
        assert!(!is_thematic_break_line("--- hello"));
        let hr = load_markdown("---\n", editor_options());
        let (_, kind) = unique_root(&hr).expect("hr");
        assert_eq!(kind, BlockKind::ThematicBreak);
        assert_eq!(close_fence_delete_range("fn\n```", 6), Some(2..6));
        assert_eq!(close_fence_delete_range("```", 3), Some(0..3));
        assert!(close_fence_delete_range("fn", 2).is_none());
        let doc = load_markdown("```rust\n", editor_options());
        let (id, kind) = unique_root(&doc).expect("root");
        assert_eq!(kind, BlockKind::CodeBlock);
        assert!(doc.extra(id).code_fence_lang().is_some());
    }

    #[test]
    fn line_range_floors_offsets_to_a_character_boundary() {
        assert_eq!(line_range("", 0), (0, 0));
        assert_eq!(line_range("a\n", 2), (2, 2));
        assert_eq!(line_range("\n\n", 1), (1, 1));
        assert_eq!(line_range("€€€", 5), (0, "€€€".len()));
    }
}
