use super::chars::floor_char_boundary;
use super::{Document, NodeId, sanitized_editor_options};
use crate::block::BlockKind;
use crate::inline::{InlineMarks, InlineRun};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::ops::Range;
const UNMAPPED: usize = usize::MAX;

pub(crate) const IMAGE_PLACEHOLDER: &str = "\u{FFFC}";

pub(crate) fn is_block_edit(kind: BlockKind) -> bool {
    kind.supports_block_edit()
}

pub(crate) fn identity_map(n: usize) -> Vec<usize> {
    (0..=n).collect()
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
        Some(rest) if rest.starts_with(' ') => &rest[1..],
        Some("") | None => "",
        Some(_) => line,
    }
}

fn heading_body_offset(source: &str) -> usize {
    let line = source.trim_end_matches(['\n', '\r']);
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return 0;
    }
    if line.as_bytes().get(hashes) == Some(&b' ') {
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
) -> (String, Vec<usize>) {
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
    }

    let lead_nls = display.chars().take_while(|&c| c == '\n').count();
    let have_ws = leading_space_tab_bytes(display.get(lead_nls..).unwrap_or(""));
    let src_body = if matches!(kind, BlockKind::Heading(_)) {
        heading_body(source)
    } else {
        source.trim_start_matches('\n')
    };
    let want_ws = leading_space_tab_bytes(src_body);
    if want_ws > have_ws
        && let Some(extra) = src_body.get(have_ws..want_ws)
    {
        let insert_at = lead_nls + have_ws;
        display.insert_str(insert_at, extra);
        let src_at = if matches!(kind, BlockKind::Heading(_)) {
            heading_body_offset(source) + have_ws
        } else {
            want_lead + have_ws
        };
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
    }
    if let Some(last) = s2d.last_mut() {
        *last = display.len();
    }
    (display, s2d)
}

fn trailing_char(s: &str, ch: char) -> usize {
    s.chars().rev().take_while(|c| *c == ch).count()
}

pub(crate) fn bind_map(source: &str, display: &str, kind: BlockKind) -> (String, Vec<usize>) {
    let source = source.trim_end_matches('\r').to_string();
    if source.is_empty() && !display.is_empty() {
        return (display.to_string(), identity_map(display.len()));
    }
    if source == display {
        let n = source.len();
        return (source, identity_map(n));
    }
    let s2d = source_to_display_map_for_kind(&source, kind);
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

pub(crate) fn source_to_display_map(source: &str) -> Vec<usize> {
    source_to_display_map_impl(source, false)
}

pub(crate) fn source_to_display_map_for_kind(source: &str, kind: BlockKind) -> Vec<usize> {
    source_to_display_map_impl(source, kind == BlockKind::TableCell)
}

fn source_to_display_map_impl(source: &str, table_soft_breaks: bool) -> Vec<usize> {
    let n = source.len();
    if n == 0 {
        return vec![0];
    }
    let mut s2d = vec![UNMAPPED; n + 1];
    let mut disp = 0usize;

    let mut image_disp_at: Option<usize> = None;
    for (event, range) in Parser::new_ext(source, sanitized_editor_options()).into_offset_iter() {
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
    let span = (hi - lo).max(1);
    let mut i = lo;
    while i <= hi {
        let rel = if i == hi {
            shown.len()
        } else {
            (i - lo) * shown.len() / span
        };
        assign(s2d, i, *disp + rel);
        i += 1;
    }
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
            let s2d = source_to_display_map(source);
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
        let s2d = source_to_display_map("`a`");
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
        let (d, s2d) = restore_visible_ws(
            BlockKind::Heading(3),
            "### title ",
            "title".into(),
            vec![0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 5],
        );
        assert_eq!(d, "title ");
        assert_eq!(*s2d.last().unwrap(), 6);
        let (d, s2d) = restore_visible_ws(
            BlockKind::Paragraph,
            "hello \n",
            "hello".into(),
            vec![0, 1, 2, 3, 4, 5, 5, 5],
        );
        assert_eq!(d, "hello \n");
        assert_eq!(*s2d.last().unwrap(), 7);
        let (d, s2d) = restore_visible_ws(
            BlockKind::Paragraph,
            "\nabc",
            "abc".into(),
            vec![0, 0, 1, 2, 3],
        );
        assert_eq!(d, "\nabc");
        assert_eq!(s2d[1], 1);
        assert_eq!(*s2d.last().unwrap(), 4);
    }

    #[test]
    fn restore_visible_ws_keeps_heading_leading_tab() {
        let src = "# \thi";
        let s2d = source_to_display_map(src);
        let (d, m) = restore_visible_ws(BlockKind::Heading(1), src, "hi".into(), s2d);
        assert_eq!(d, "\thi");
        assert_eq!(*m.last().unwrap(), d.len());
        assert!(m.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(display_to_source_inner(&m, 0), 2);
    }

    #[test]
    fn restore_visible_ws_keeps_typed_atx_closer() {
        let src = "# hello #";
        let s2d = source_to_display_map(src);
        let (d, m) = restore_visible_ws(BlockKind::Heading(1), src, "hello".into(), s2d);
        assert_eq!(d, "hello #");
        assert_eq!(*m.last().unwrap(), d.len());
        assert_eq!(display_to_source_inner(&m, d.len()), src.len());

        let src = "# #";
        let s2d = source_to_display_map(src);
        let (d, m) = restore_visible_ws(BlockKind::Heading(1), src, String::new(), s2d);
        assert_eq!(d, "#");
        assert_eq!(*m.last().unwrap(), d.len());
        assert_eq!(display_to_source_inner(&m, d.len()), src.len());
    }

    #[test]
    fn restore_visible_ws_keeps_paragraph_leading_spaces() {
        let src = "  hello";
        let s2d = source_to_display_map(src);
        let (d, m) = restore_visible_ws(BlockKind::Paragraph, src, "hello".into(), s2d);
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
            let s2d = source_to_display_map(src);
            let (d, m) =
                restore_visible_ws(BlockKind::Paragraph, src, frag.display(id).to_string(), s2d);
            assert_eq!(d, want, "src={src:?}");
            assert_eq!(*m.last().unwrap(), d.len(), "src={src:?}");
        }
    }
}
