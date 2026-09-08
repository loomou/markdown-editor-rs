use super::chars::floor_char_boundary;
use super::{Document, NodeId, sanitized_editor_options};
use crate::block::BlockKind;
use crate::inline::{InlineMarks, InlineRun};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::ops::Range;
const UNMAPPED: usize = usize::MAX;

pub(crate) const IMAGE_PLACEHOLDER: &str = "\u{FFFC}";

pub(crate) fn is_block_edit(kind: BlockKind) -> bool {
    kind.supports_block_edit()
}

pub(crate) fn identity_map(n: usize) -> Vec<usize> {
    (0..=n).collect()
}

pub(crate) fn escape_literal(out: &mut String, text: &str) {
    let mut at_line_start = out.is_empty() || out.ends_with('\n');
    let mut lead_digits = false;
    for c in text.chars() {
        let escapes = matches!(
            c,
            '\\' | '*'
                | '_'
                | '~'
                | '`'
                | '['
                | ']'
                | '<'
                | '!'
                | '#'
                | '>'
                | '&'
                | '+'
                | '-'
                | '$'
                | '^'
        );
        let lead = at_line_start && (c == '.' || c == ')' || c == '=');
        let marker = lead_digits && (c == '.' || c == ')');
        if escapes || lead || marker {
            out.push('\\');
        }
        out.push(c);
        lead_digits = (at_line_start || lead_digits) && c.is_ascii_digit();
        at_line_start = c == '\n';
    }
}

pub(crate) fn identity_runs(n: usize) -> Vec<InlineRun> {
    if n == 0 {
        Vec::new()
    } else {
        vec![InlineRun {
            display_range: 0..n as u32,
            source_range: None,
            marks: InlineMarks::NONE,
            link: None,
        }]
    }
}

pub(crate) fn unique_root(frag: &Document) -> Option<(NodeId, BlockKind)> {
    let mut kids = frag.arena.children(frag.root);
    let root = kids.next()?;
    if kids.next().is_some() {
        return None;
    }
    let kind = frag.arena.get(root).map(|n| n.kind)?;
    Some((root, kind))
}
pub(crate) fn quote_lead_paragraph(frag: &Document) -> Option<Option<NodeId>> {
    let (quote, kind) = unique_root(frag)?;
    if kind != BlockKind::BlockQuote {
        return None;
    }
    let mut kids = frag.arena.children(quote);
    let lead = kids.next();
    if kids.next().is_some() {
        return None;
    }
    match lead {
        None => Some(None),
        Some(id) => {
            if frag.arena.get(id).map(|n| n.kind) == Some(BlockKind::Paragraph) {
                Some(Some(id))
            } else {
                None
            }
        }
    }
}

pub(crate) fn matching_leaf(frag: &Document, current: BlockKind) -> Option<NodeId> {
    let mut kids = frag.arena.children(frag.root);
    let root = kids.next()?;
    if kids.next().is_some() {
        return None;
    }
    let kind = frag.arena.get(root).map(|n| n.kind)?;
    if kind != current || !kind.is_text_leaf() {
        return None;
    }
    Some(root)
}
pub(crate) fn heading_body(source: &str) -> &str {
    let line = source.trim_end_matches(['\n', '\r']);
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return source;
    }
    match line.get(hashes..) {
        Some(rest) if rest.starts_with(' ') || rest.starts_with('\t') => &rest[1..],
        Some("") | None => "",
        Some(_) => line,
    }
}

pub(crate) fn setext_body(source: &str) -> Option<&str> {
    let trimmed = source.trim_end_matches(['\n', '\r']);
    let last_line_start = trimmed.rfind('\n').map(|i| i + 1).unwrap_or(0);
    if last_line_start == 0 {
        return None;
    }
    let body_end = last_line_start - 1;
    let sep = trimmed[last_line_start..].trim_matches([' ', '\t']);
    if sep.is_empty() || !(sep.chars().all(|c| c == '=') || sep.chars().all(|c| c == '-')) {
        return None;
    }
    Some(&trimmed[..body_end])
}

fn heading_body_offset(source: &str) -> usize {
    let line = source.trim_end_matches(['\n', '\r']);
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return 0;
    }
    if matches!(line.as_bytes().get(hashes), Some(&b' ') | Some(&b'\t')) {
        hashes + 1
    } else {
        0
    }
}

fn leading_space_tab_bytes(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b' ' || b == b'\t').count()
}

fn atx_closer_suffix(body: &str) -> &str {
    let bytes = body.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1] == b'#' {
        i -= 1;
    }
    if i == bytes.len() {
        return "";
    }
    let hashes = i;
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    if i == hashes {
        return "";
    }
    &body[i..]
}

fn map_source_span(s2d: &mut [usize], src_at: usize, display_at: usize, len: usize) {
    for i in 0..=len {
        if let Some(slot) = s2d.get_mut(src_at + i) {
            *slot = display_at + i;
        }
    }
}

fn restore_atx_closing_hashes(source: &str, display: &mut String, s2d: &mut [usize]) {
    let body = heading_body(source);
    if body.is_empty() {
        return;
    }
    let visible = body.trim_end_matches([' ', '\t']);
    if visible.is_empty() {
        return;
    }
    if display.is_empty() {
        display.push_str(visible);
        map_source_span(s2d, heading_body_offset(source), 0, visible.len());
        return;
    }
    let closer = atx_closer_suffix(visible);
    if closer.is_empty() || display.ends_with(closer) {
        return;
    }
    let insert_at = display.len();
    display.push_str(closer);
    let line = source.trim_end_matches(['\n', '\r']);
    let trail = body.len() - visible.len();
    let src_at = line.len().saturating_sub(trail + closer.len());
    map_source_span(s2d, src_at, insert_at, closer.len());
}

pub(crate) fn restore_visible_ws(
    kind: BlockKind,
    source: &str,
    mut display: String,
    mut s2d: Vec<usize>,
    mut runs: Vec<InlineRun>,
) -> (String, Vec<usize>, Vec<InlineRun>) {
    if matches!(kind, BlockKind::Heading(_)) {
        restore_atx_closing_hashes(source, &mut display, &mut s2d);
    }
    let content = if matches!(kind, BlockKind::Heading(_)) {
        heading_body(source)
    } else {
        source
    };
    let want_spaces = trailing_char(content.trim_end_matches(['\n', '\r']), ' ');
    let want_nls = trailing_char(source, '\n');
    let want_lead = source.chars().take_while(|&c| c == '\n').count();
    let have_lead = display.chars().take_while(|&c| c == '\n').count();
    let add_lead = want_lead.saturating_sub(have_lead);
    let core_end = display.trim_end_matches([' ', '\n', '\r']).len();
    let tail = &display[core_end..];
    let tail_spaces = trailing_char(tail.trim_end_matches(['\n', '\r']), ' ');
    let tail_nls = trailing_char(tail, '\n');
    display.truncate(core_end);
    display.extend(std::iter::repeat_n(' ', tail_spaces.max(want_spaces)));
    display.extend(std::iter::repeat_n('\n', tail_nls.max(want_nls)));
    if add_lead > 0 {
        display.insert_str(0, &"\n".repeat(add_lead));
        for slot in &mut s2d {
            *slot = slot.saturating_add(add_lead);
        }
        for (i, slot) in s2d.iter_mut().enumerate().take(add_lead + 1) {
            *slot = i;
        }
        shift_run_displays(&mut runs, 0, add_lead);
    }
    let src_body = if matches!(kind, BlockKind::Heading(_)) {
        heading_body(source)
    } else {
        source.trim_start_matches('\n')
    };
    let lead_nls = display.chars().take_while(|&c| c == '\n').count();
    let src_lines: Vec<&str> = src_body.split('\n').collect();
    let mut display_lines: Vec<String> = display
        .get(lead_nls..)
        .unwrap_or("")
        .split('\n')
        .map(str::to_string)
        .collect();
    if src_lines.len() == display_lines.len() {
        let mut grown = 0usize;
        let base_offset = if matches!(kind, BlockKind::Heading(_)) {
            heading_body_offset(source)
        } else {
            want_lead
        };
        let mut src_pos = 0usize;
        let mut display_pos = lead_nls;
        for (line_idx, (src_line, display_line)) in
            src_lines.iter().zip(display_lines.iter_mut()).enumerate()
        {
            let want = leading_space_tab_bytes(src_line);
            let have = leading_space_tab_bytes(display_line);
            let _ = line_idx;
            if want > have
                && let Some(extra) = src_line.get(have..want)
            {
                display_line.insert_str(have, extra);
                let insert_at = display_pos + have;
                let src_at = base_offset + src_pos + have;
                for slot in s2d.iter_mut().skip(src_at) {
                    if *slot >= insert_at {
                        *slot = slot.saturating_add(extra.len());
                    }
                }
                for i in 0..=extra.len() {
                    if let Some(slot) = s2d.get_mut(src_at + i) {
                        *slot = insert_at + i;
                    }
                }
                shift_run_displays(&mut runs, insert_at, extra.len());
                grown += extra.len();
            }
            src_pos += src_line.len() + 1;
            display_pos += display_line.len() + 1;
        }
        if grown > 0 {
            let joined = display_lines.join("\n");
            let mut next = String::with_capacity(lead_nls + joined.len());
            next.push_str(&display[..lead_nls]);
            next.push_str(&joined);
            display = next;
        }
    }
    if let Some(last) = s2d.last_mut() {
        *last = display.len();
    }
    (display, s2d, runs)
}

fn shift_run_displays(runs: &mut [InlineRun], at: usize, by: usize) {
    if by == 0 {
        return;
    }
    let at = at as u32;
    let by = by as u32;
    for run in runs.iter_mut() {
        if run.display_range.start >= at {
            run.display_range.start += by;
        }
        if run.display_range.end >= at {
            run.display_range.end += by;
        }
    }
}

fn trailing_char(s: &str, ch: char) -> usize {
    s.chars().rev().take_while(|c| *c == ch).count()
}

pub(crate) fn bind_map(
    source: &str,
    display: &str,
    kind: BlockKind,
    definitions: &[String],
) -> (String, Vec<usize>) {
    let source = source.trim_end_matches('\r').to_string();
    if source.is_empty() && !display.is_empty() {
        return (display.to_string(), identity_map(display.len()));
    }
    if source == display {
        let n = source.len();
        return (source, identity_map(n));
    }
    let s2d = source_to_display_map_impl(&source, kind == BlockKind::TableCell, definitions);
    (source, s2d)
}

pub(crate) fn is_html_line_break(html: &str) -> bool {
    let html = html.trim();
    html.eq_ignore_ascii_case("<br>")
        || html.eq_ignore_ascii_case("<br/>")
        || html.eq_ignore_ascii_case("<br />")
}

pub(crate) fn html_line_break_before(source: &str, at: usize) -> Option<Range<usize>> {
    let at = floor_char_boundary(source, at.min(source.len()));
    for len in [6, 5, 4] {
        let Some(start) = at.checked_sub(len) else {
            continue;
        };
        if is_html_line_break(source.get(start..at).unwrap_or("")) {
            return Some(start..at);
        }
    }
    None
}

pub(crate) fn source_to_display(s2d: &[usize], s: usize) -> usize {
    let i = s.min(s2d.len().saturating_sub(1));
    s2d.get(i).copied().unwrap_or(0)
}

pub(crate) fn display_to_source_inner(s2d: &[usize], d: usize) -> usize {
    if s2d.is_empty() {
        return 0;
    }
    let max_d = *s2d.last().unwrap_or(&0);
    let d = d.min(max_d);
    let last = s2d.partition_point(|&md| md <= d).saturating_sub(1);
    if s2d[last] == d {
        last
    } else {
        display_to_source_outer(s2d, d)
    }
}

pub(crate) fn display_to_source_outer(s2d: &[usize], d: usize) -> usize {
    s2d.partition_point(|&md| md <= d).saturating_sub(1)
}

pub(crate) fn display_to_source_first(s2d: &[usize], d: usize) -> usize {
    s2d.partition_point(|&md| md < d)
        .min(s2d.len().saturating_sub(1))
}

pub(crate) fn source_span<'a>(
    source: &'a str,
    s2d: &[usize],
    rs: usize,
    re: usize,
) -> Option<&'a str> {
    let a = display_to_source_first(s2d, rs);
    let b = display_to_source_outer(s2d, re);
    let a = floor_char_boundary(source, a.min(source.len()));
    let b = floor_char_boundary(source, b.min(source.len())).max(a);
    if a >= b {
        return None;
    }
    source.get(a..b)
}

pub(crate) fn source_to_display_map(source: &str, definitions: &[String]) -> Vec<usize> {
    source_to_display_map_impl(source, false, definitions)
}

pub(crate) fn source_to_display_map_for_kind(
    source: &str,
    kind: BlockKind,
    definitions: &[String],
) -> Vec<usize> {
    if kind != BlockKind::TableCell {
        return source_to_display_map_impl(source, false, definitions);
    }
    let defs: &[String] = if source.contains('\n') {
        &[]
    } else {
        definitions
    };
    source_to_display_map_impl(source, true, defs)
}

pub(crate) fn with_definitions<'a>(source: &'a str, definitions: &[String]) -> Cow<'a, str> {
    if definitions.is_empty() {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(format!("{source}\n\n{}", definitions.join("\n")))
    }
}

fn needs_definitions(source: &str) -> bool {
    !referenced_labels(source).is_empty()
}

fn referenced_labels(source: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut at = 0;
    while let Some(open) = source[at..].find('[') {
        let open = at + open;
        let Some(close_rel) = source[open + 1..].find(']') else {
            break;
        };
        let close = open + 1 + close_rel;
        let candidate = &source[open + 1..close];
        let label = if candidate.starts_with('^') || source.as_bytes().get(close + 1) == Some(&b'(')
        {
            None
        } else {
            let next = source[close + 1..].chars().next();
            match next {
                Some('[') => {
                    let second_open = close + 2;
                    match source[second_open..].find(']') {
                        Some(rel) if rel > 0 => Some(&source[second_open..second_open + rel]),
                        Some(_) => Some(candidate),
                        None => None,
                    }
                }
                _ => Some(candidate),
            }
        };
        if let Some(label) = label {
            let key = normalize_label(label);
            if !key.is_empty() && !labels.iter().any(|l: &String| l == &key) {
                labels.push(key);
            }
        }
        at = close + 1;
    }
    labels
}

fn normalize_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut pending_space = false;
    for ch in label.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            for folded in ch.to_lowercase() {
                out.push(folded);
            }
        }
    }
    out
}

fn definition_matches_any(definition: &str, labels: &[String]) -> bool {
    let body = definition.trim();
    let Some(rest) = body.strip_prefix('[') else {
        return false;
    };
    let Some(close) = rest.find(']') else {
        return false;
    };
    let label = normalize_label(&rest[..close]);
    !label.is_empty() && labels.iter().any(|l| l == &label)
}

fn source_to_display_map_impl(
    source: &str,
    table_soft_breaks: bool,
    definitions: &[String],
) -> Vec<usize> {
    let n = source.len();
    if n == 0 {
        return vec![0];
    }
    let mut s2d = vec![UNMAPPED; n + 1];
    let mut disp = 0usize;
    let mut image_disp_at: Option<usize> = None;
    let owned;
    let parse_source: &str = if definitions.is_empty() || !needs_definitions(source) {
        source
    } else {
        let used = referenced_labels(source);
        let mut relevant = String::new();
        for definition in definitions {
            if definition_matches_any(definition, &used) {
                relevant.push_str(definition);
                relevant.push('\n');
            }
        }
        if relevant.is_empty() {
            source
        } else {
            owned = format!("{source}\n\n{relevant}");
            &owned
        }
    };
    for (event, range) in
        Parser::new_ext(parse_source, sanitized_editor_options()).into_offset_iter()
    {
        if range.start > n {
            continue;
        }
        let lo = range.start.min(n);
        let hi = range.end.min(n).max(lo);
        match event {
            Event::Start(Tag::Image { .. }) => {
                image_disp_at = Some(disp);
            }
            Event::End(TagEnd::Image) => {
                if image_disp_at.take() == Some(disp) {
                    fill(&mut s2d, lo, hi, disp);
                    disp = disp.saturating_add(IMAGE_PLACEHOLDER.len());
                    assign(&mut s2d, hi, disp);
                }
            }
            Event::Start(_) | Event::End(_) => {}
            Event::Text(t) => {
                map_shown(source, &mut s2d, lo, hi, t.as_ref(), &mut disp);
            }
            Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                map_shown(source, &mut s2d, lo, hi, t.as_ref(), &mut disp);
            }
            Event::SoftBreak => {
                fill(&mut s2d, lo, hi, disp);
                disp = disp.saturating_add(1);
                assign(&mut s2d, hi, disp);
            }
            Event::HardBreak => {
                fill(&mut s2d, lo, hi, disp);
                disp = disp.saturating_add(1);
                assign(&mut s2d, hi, disp);
            }
            Event::FootnoteReference(t) => {
                let shown = format!("[^{t}]");
                map_shown(source, &mut s2d, lo, hi, &shown, &mut disp);
            }
            Event::Html(t) | Event::InlineHtml(t)
                if table_soft_breaks && is_html_line_break(&t) =>
            {
                assign(&mut s2d, lo, disp);
                disp = disp.saturating_add(1);
                fill(&mut s2d, lo.saturating_add(1), hi, disp);
            }
            _ => {}
        }
    }
    fill_gaps(&mut s2d);
    s2d
}

fn map_shown(source: &str, s2d: &mut [usize], lo: usize, hi: usize, shown: &str, disp: &mut usize) {
    let slice = source.get(lo..hi).unwrap_or("");
    if slice == shown {
        fill_1to1(s2d, lo, shown.len(), *disp);
        *disp += shown.len();
        return;
    }
    if !shown.is_empty()
        && let Some(rel) = slice.find(shown)
    {
        let inner_lo = lo + rel;
        let inner_hi = inner_lo + shown.len();
        fill(s2d, lo, inner_lo, *disp);
        fill_1to1(s2d, inner_lo, shown.len(), *disp);
        *disp += shown.len();
        fill(s2d, inner_hi, hi, *disp);
        return;
    }
    if shown.is_empty() {
        fill(s2d, lo, hi, *disp);
        return;
    }
    let src_chars = slice.chars().count();
    let shown_chars = shown.chars().count();
    if src_chars == shown_chars {
        let mut si = lo;
        let mut d = *disp;
        for (src_ch, shown_ch) in slice.chars().zip(shown.chars()) {
            assign(s2d, si, d);
            si += src_ch.len_utf8();
            d += shown_ch.len_utf8();
        }
        assign(s2d, hi, d);
        *disp = d;
        return;
    }
    assign(s2d, lo, *disp);
    fill(s2d, lo + 1, hi, *disp + shown.len());
    *disp += shown.len();
}

fn fill(s2d: &mut [usize], lo: usize, hi: usize, d: usize) {
    let n = s2d.len().saturating_sub(1);
    let lo = lo.min(n);
    let hi = hi.min(n);
    if lo <= hi {
        for slot in &mut s2d[lo..=hi] {
            *slot = d;
        }
    }
}

fn fill_1to1(s2d: &mut [usize], src_lo: usize, len: usize, disp: usize) {
    let n = s2d.len().saturating_sub(1);
    for i in 0..=len {
        assign(s2d, (src_lo + i).min(n), disp + i);
    }
}

fn assign(s2d: &mut [usize], i: usize, d: usize) {
    if i < s2d.len() {
        s2d[i] = d;
    }
}

fn fill_gaps(s2d: &mut [usize]) {
    let mut last = 0;
    for slot in s2d.iter_mut() {
        if *slot == UNMAPPED {
            *slot = last;
        } else {
            last = *slot;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{editor_options, load_markdown};
    use super::{
        display_to_source_inner, display_to_source_outer, heading_body, restore_visible_ws,
        source_to_display, source_to_display_map,
    };
    use crate::block::BlockKind;

    #[test]
    fn source_map_len_matches_cold_display() {
        for src in [
            "hello",
            "`a`",
            "*b*",
            "**c**",
            "~~d~~",
            "hello `d` and *em*",
            "hello\nworld",
            "- a",
            "# x",
            "**a** *b* ~~c~~ `d` [e](https://ex) ^s^ ~u~",
            "![](u) x",
            "x ![](u)",
            "x ![](u) y",
            "![](u)![](v)",
            "![a](u) x",
            "x [![](u)](l) y",
        ] {
            let doc = load_markdown(src, editor_options());
            let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
            let source = doc.leaf_source(id);
            let s2d = source_to_display_map(source, &[]);
            assert_eq!(
                *s2d.last().unwrap(),
                doc.display(id).len(),
                "src={src:?} leaf_source={source:?} map={s2d:?} display={:?}",
                doc.display(id)
            );
        }
    }

    #[test]
    fn closed_code_maps_caret_inside_span() {
        let s2d = source_to_display_map("`a`", &[]);
        assert_eq!(s2d, vec![0, 0, 1, 1]);
        assert_eq!(display_to_source_inner(&s2d, 0), 1);
        assert_eq!(display_to_source_inner(&s2d, 1), 3);
        assert_eq!(display_to_source_outer(&s2d, 1), 3);
        assert_eq!(source_to_display(&s2d, 3), 1);
    }

    #[test]
    fn heading_body_strips_atx_marker() {
        assert_eq!(heading_body("# "), "");
        assert_eq!(heading_body("#  "), " ");
        assert_eq!(heading_body("### hello "), "hello ");
        assert_eq!(heading_body("### title"), "title");
        assert_eq!(heading_body("hello"), "hello");
    }

    #[test]
    fn restore_visible_ws_keeps_spaces_and_newline() {
        let (d, s2d, _) = restore_visible_ws(
            BlockKind::Heading(3),
            "### title ",
            "title".into(),
            vec![0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 5],
            Vec::new(),
        );
        assert_eq!(d, "title ");
        assert_eq!(*s2d.last().unwrap(), 6);
        let (d, s2d, _) = restore_visible_ws(
            BlockKind::Paragraph,
            "hello \n",
            "hello".into(),
            vec![0, 1, 2, 3, 4, 5, 5, 5],
            Vec::new(),
        );
        assert_eq!(d, "hello \n");
        assert_eq!(*s2d.last().unwrap(), 7);
        let (d, s2d, _) = restore_visible_ws(
            BlockKind::Paragraph,
            "\nabc",
            "abc".into(),
            vec![0, 0, 1, 2, 3],
            Vec::new(),
        );
        assert_eq!(d, "\nabc");
        assert_eq!(s2d[1], 1);
        assert_eq!(*s2d.last().unwrap(), 4);
    }

    #[test]
    fn restore_visible_ws_keeps_heading_leading_tab() {
        let src = "# \thi";
        let s2d = source_to_display_map(src, &[]);
        let (d, m, _) =
            restore_visible_ws(BlockKind::Heading(1), src, "hi".into(), s2d, Vec::new());
        assert_eq!(d, "\thi");
        assert_eq!(*m.last().unwrap(), d.len());
        assert!(m.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(display_to_source_inner(&m, 0), 2);
    }

    #[test]
    fn restore_visible_ws_keeps_typed_atx_closer() {
        let src = "# hello #";
        let s2d = source_to_display_map(src, &[]);
        let (d, m, _) =
            restore_visible_ws(BlockKind::Heading(1), src, "hello".into(), s2d, Vec::new());
        assert_eq!(d, "hello #");
        assert_eq!(*m.last().unwrap(), d.len());
        assert_eq!(display_to_source_inner(&m, d.len()), src.len());

        let src = "# #";
        let s2d = source_to_display_map(src, &[]);
        let (d, m, _) =
            restore_visible_ws(BlockKind::Heading(1), src, String::new(), s2d, Vec::new());
        assert_eq!(d, "#");
        assert_eq!(*m.last().unwrap(), d.len());
        assert_eq!(display_to_source_inner(&m, d.len()), src.len());
    }

    #[test]
    fn restore_visible_ws_keeps_paragraph_leading_spaces() {
        let src = "  hello";
        let s2d = source_to_display_map(src, &[]);
        let (d, m, _) =
            restore_visible_ws(BlockKind::Paragraph, src, "hello".into(), s2d, Vec::new());
        assert_eq!(d, "  hello");
        assert_eq!(*m.last().unwrap(), d.len());
        assert_eq!(display_to_source_inner(&m, 0), 0);
    }

    #[test]
    fn list_item_leaf_source_excludes_marker() {
        let doc = load_markdown("- a\n- b\n", editor_options());
        let leaves = doc.text_leaves();
        let a = doc.live_id(leaves[0]).expect("a");
        let b = doc.live_id(leaves[1]).expect("b");
        assert_eq!(doc.leaf_source(a), "a");
        assert_eq!(doc.leaf_source(b), "b");
        assert_eq!(doc.display(a), "a");
        assert_eq!(doc.display(b), "b");
    }

    #[test]
    fn restore_visible_ws_keeps_parser_visible_trailing_ws() {
        for (src, want) in [("`a `", "a "), ("`a  `", "a  "), ("`a `b", "a b")] {
            let frag = load_markdown(src, editor_options());
            let id = frag.live_id(frag.text_leaves()[0]).expect("leaf");
            let s2d = source_to_display_map(src, &[]);
            let (d, m, _) = restore_visible_ws(
                BlockKind::Paragraph,
                src,
                frag.display(id).to_string(),
                s2d,
                Vec::new(),
            );
            assert_eq!(d, want, "src={src:?}");
            assert_eq!(*m.last().unwrap(), d.len(), "src={src:?}");
        }
    }
}
