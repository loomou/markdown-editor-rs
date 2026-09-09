use super::span::{clear_same_block_span, insert_span};
use super::{Caret, Command, Sel, apply, list, path};
use crate::block::BlockKind;
use crate::document::{
    Document, PasteIntent, bind, editor_options, floor_char_boundary, load_markdown,
};

pub(super) fn insert(doc: &mut Document, sel: Sel, text: &str) -> Caret {
    let sel = if sel.anchor.block != sel.head.block {
        let at = clear_same_block_span(doc, sel);
        Sel::collapsed(at)
    } else {
        sel
    };
    let (block, range) = insert_span(sel);
    let (block, range) = match doc.kind(block) {
        Some(k) if k.is_text_leaf() => (block, range),
        _ => match doc.first_text_leaf() {
            Some(leaf) => (leaf, 0..0),
            None => return sel.head,
        },
    };
    let intent = if text.contains('\n')
        && doc.kind(block) == Some(BlockKind::Paragraph)
        && batched_insert_is_structural(text)
    {
        PasteIntent::IndependentFragment
    } else {
        PasteIntent::PlainText
    };
    let (_, block, offset) = doc.paste(block, range, text, intent);
    after_paragraph_insert(doc, Caret { block, offset })
}

fn batched_insert_is_structural(text: &str) -> bool {
    let frag = load_markdown(text, editor_options());
    frag.arena.children(frag.root).any(|id| {
        !matches!(
            frag.arena.get(id).map(|n| n.kind),
            Some(BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::Image)
        )
    })
}

pub(super) fn after_paragraph_insert(doc: &mut Document, caret: Caret) -> Caret {
    if doc.kind(caret.block) != Some(BlockKind::Paragraph) {
        return caret;
    }
    let Some(leaf) = doc.live_id(caret.block) else {
        return caret;
    };
    if let Some(caret) = doc.try_commit_atx_heading(leaf, caret.offset) {
        return caret;
    }
    if let Some(caret) = doc.try_commit_block_quote(leaf, caret.offset) {
        return caret;
    }
    if let Some(caret) = try_wrap_quote_line(doc, caret) {
        return caret;
    }
    if let Some(caret) = list::try_commit_task(doc, caret) {
        return caret;
    }
    if let Some(path) = path::Path::at(doc, caret)
        && path.item.is_some()
    {
        return caret;
    }
    if let Some(caret) = try_wrap_marker_line(doc, caret) {
        return caret;
    }
    let Some((ordered, task)) = list::marker_spec(doc.display(leaf)) else {
        return caret;
    };
    apply(
        doc,
        Sel::collapsed(caret),
        Command::WrapList { ordered, task },
    )
}

pub(super) fn split_marker_line(doc: &mut Document, caret: Caret, line: &str) -> Option<Caret> {
    let id = doc.live_id(caret.block)?;
    let text = doc.caret_text(id);
    let (a, b) = crate::document::syntax::line_range(text, caret.offset);
    if text.get(a..b)? != line {
        return None;
    }
    if a == 0 || text.as_bytes().get(a - 1) != Some(&b'\n') {
        return None;
    }
    let source = doc.leaf_source(id);
    let s2d = doc.visual_s2d(id);
    let source_a = bind::display_to_source_first(&s2d, a);
    let source_b = bind::display_to_source_inner(&s2d, b);
    let source_a = floor_char_boundary(source, source_a.min(source.len()));
    let source_b = floor_char_boundary(source, source_b.min(source.len())).max(source_a);
    let source_line = source.get(source_a..source_b)?.to_string();
    let block = caret.block;
    let marker_block = if a == 0 {
        block
    } else {
        let (_, marker_block) = doc.split_leaf(block, a - 1);
        if doc
            .text_of(marker_block)
            .is_some_and(|t| t.starts_with('\n'))
        {
            let _ = doc.replace_text(marker_block, 0..1, "");
        }
        marker_block
    };
    let marker_text = doc.text_of(marker_block)?;
    let marker_end = crate::document::syntax::line_range(marker_text, 0).1;
    if marker_end < marker_text.len() {
        let split_at =
            marker_end + usize::from(marker_text.as_bytes().get(marker_end) == Some(&b'\n'));
        let _ = doc.split_leaf(marker_block, split_at);
    }
    let _ = doc.replace_text(marker_block, 0..marker_end, &source_line);
    let offset = doc
        .live_id(marker_block)
        .map(|id| doc.caret_text(id).len())
        .unwrap_or(source_line.len());
    Some(Caret {
        block: marker_block,
        offset,
    })
}

fn source_line_of(
    doc: &Document,
    id: crate::document::arena::NodeId,
    offset: usize,
) -> Option<String> {
    let text = doc.caret_text(id);
    let (a, b) = crate::document::syntax::line_range(text, offset);
    let source = doc.leaf_source(id);
    let s2d = doc.visual_s2d(id);
    let source_a = floor_char_boundary(
        source,
        bind::display_to_source_first(&s2d, a).min(source.len()),
    );
    let source_b = floor_char_boundary(
        source,
        bind::display_to_source_inner(&s2d, b).min(source.len()),
    )
    .max(source_a);
    source.get(source_a..source_b).map(str::to_string)
}

pub(super) fn try_wrap_quote_line(doc: &mut Document, caret: Caret) -> Option<Caret> {
    let text = doc.text_of(caret.block)?;
    let (a, b) = crate::document::syntax::line_range(text, caret.offset);
    let line = text.get(a..b)?.to_string();
    if !crate::document::syntax::is_quote_commit(&line) {
        return None;
    }
    let id = doc.live_id(caret.block)?;
    let source_line = source_line_of(doc, id, caret.offset)?;
    if !crate::document::syntax::is_quote_commit(&source_line) {
        return None;
    }
    let at = split_marker_line(doc, caret, &line)?;
    let leaf = doc.live_id(at.block)?;
    doc.try_commit_block_quote(leaf, at.offset)
}

pub(super) fn try_wrap_marker_line(doc: &mut Document, caret: Caret) -> Option<Caret> {
    let text = doc.text_of(caret.block)?;
    let (a, b) = crate::document::syntax::line_range(text, caret.offset);
    let line = text.get(a..b)?.to_string();
    let (_, ordered, task) = list::marker_prefix_spec(&line)?;
    let id = doc.live_id(caret.block)?;
    let source_line = source_line_of(doc, id, caret.offset)?;
    list::marker_prefix_spec(&source_line)?;
    let at = split_marker_line(doc, caret, &line)?;
    Some(apply(
        doc,
        Sel::collapsed(at),
        Command::WrapList { ordered, task },
    ))
}
