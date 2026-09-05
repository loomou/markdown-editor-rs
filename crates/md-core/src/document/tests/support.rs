use crate::document::Document;

pub(super) fn kind_count(doc: &Document, kind: crate::block::BlockKind) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(kind))
        .count()
}
