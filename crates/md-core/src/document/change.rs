use super::arena::NodeId;
use crate::block::{BlockKind, NodeExtra};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocChange {
    DocumentReplaced,
    TextChanged {
        node: NodeId,
        old_revision: u64,
        new_revision: u64,
        range: Range<u32>,
        deleted: String,
        inserted: String,
    },

    TreeSpliced {
        parent: NodeId,
        before: Option<NodeId>,
        removed: Vec<NodeId>,
        inserted: Vec<NodeId>,
    },
    AttrsChanged {
        node: NodeId,
        old_kind: BlockKind,
        new_kind: BlockKind,
        old_extra: NodeExtra,
        new_extra: NodeExtra,
    },
}

impl DocChange {
    pub(crate) fn text(
        node: NodeId,
        old_revision: u64,
        new_revision: u64,
        range: Range<u32>,
        deleted: impl Into<String>,
        inserted: impl Into<String>,
    ) -> Self {
        DocChange::TextChanged {
            node,
            old_revision,
            new_revision,
            range,
            deleted: deleted.into(),
            inserted: inserted.into(),
        }
    }

    pub(crate) fn attrs(
        node: NodeId,
        old_kind: BlockKind,
        new_kind: BlockKind,
        old_extra: NodeExtra,
        new_extra: NodeExtra,
    ) -> Self {
        DocChange::AttrsChanged {
            node,
            old_kind,
            new_kind,
            old_extra,
            new_extra,
        }
    }

    fn invert(&self) -> Self {
        match self.clone() {
            DocChange::DocumentReplaced => DocChange::DocumentReplaced,
            DocChange::TextChanged {
                node,
                old_revision,
                new_revision,
                range,
                deleted,
                inserted,
            } => {
                let start = range.start;
                let end = start.saturating_add(inserted.len() as u32);
                DocChange::TextChanged {
                    node,
                    old_revision: new_revision,
                    new_revision: old_revision,
                    range: start..end,
                    deleted: inserted,
                    inserted: deleted,
                }
            }
            DocChange::TreeSpliced {
                parent,
                before,
                removed,
                inserted,
            } => DocChange::TreeSpliced {
                parent,
                before,
                removed: inserted,
                inserted: removed,
            },
            DocChange::AttrsChanged {
                node,
                old_kind,
                new_kind,
                old_extra,
                new_extra,
            } => DocChange::AttrsChanged {
                node,
                old_kind: new_kind,
                new_kind: old_kind,
                old_extra: new_extra,
                new_extra: old_extra,
            },
        }
    }

    fn records_edit(&self) -> bool {
        match self {
            DocChange::DocumentReplaced => true,
            DocChange::TextChanged {
                deleted, inserted, ..
            } => deleted != inserted,
            DocChange::TreeSpliced {
                removed, inserted, ..
            } => removed != inserted,
            DocChange::AttrsChanged {
                old_kind,
                new_kind,
                old_extra,
                new_extra,
                ..
            } => old_kind != new_kind || old_extra != new_extra,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSet {
    pub before_revision: u64,
    pub after_revision: u64,
    pub changes: Vec<DocChange>,
}

impl ChangeSet {
    pub fn empty(revision: u64) -> Self {
        ChangeSet {
            before_revision: revision,
            after_revision: revision,
            changes: Vec::new(),
        }
    }

    pub(crate) fn document_replaced(after_revision: u64) -> Self {
        ChangeSet {
            before_revision: 0,
            after_revision,
            changes: vec![DocChange::DocumentReplaced],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn is_structural(&self) -> bool {
        self.changes.iter().any(|c| {
            matches!(
                c,
                DocChange::DocumentReplaced | DocChange::TreeSpliced { .. }
            )
        })
    }

    pub fn is_text_only(&self) -> bool {
        !self.changes.is_empty()
            && self
                .changes
                .iter()
                .all(|c| matches!(c, DocChange::TextChanged { .. }))
    }

    pub fn is_replace(&self) -> bool {
        self.changes
            .iter()
            .any(|c| matches!(c, DocChange::DocumentReplaced))
    }

    pub(crate) fn invert(&self) -> Self {
        ChangeSet {
            before_revision: self.after_revision,
            after_revision: self.before_revision,
            changes: self.changes.iter().rev().map(DocChange::invert).collect(),
        }
    }

    pub(crate) fn records_edit(&self) -> bool {
        !self.is_replace() && self.changes.iter().any(DocChange::records_edit)
    }

    pub(crate) fn prepend(&mut self, other: ChangeSet) {
        let mut changes = other.changes;
        changes.append(&mut self.changes);
        self.changes = changes;
        self.before_revision = other.before_revision;
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangeSet, DocChange};
    use crate::block::{BlockKind, NodeExtra};
    use crate::document::arena::NodeId;

    fn nid(i: u32) -> NodeId {
        NodeId::at(i, 1)
    }

    #[test]
    fn invert_twice_is_identity_for_text() {
        let cs = ChangeSet {
            before_revision: 1,
            after_revision: 2,
            changes: vec![DocChange::text(nid(1), 1, 2, 0..1, "a", "bc")],
        };
        assert_eq!(cs.invert().invert(), cs);
    }

    #[test]
    fn invert_swaps_splice_and_attrs() {
        let cs = ChangeSet {
            before_revision: 3,
            after_revision: 4,
            changes: vec![
                DocChange::attrs(
                    nid(2),
                    BlockKind::Paragraph,
                    BlockKind::Heading(1),
                    NodeExtra::None,
                    NodeExtra::None,
                ),
                DocChange::TreeSpliced {
                    parent: nid(0),
                    before: Some(nid(1)),
                    removed: vec![nid(3)],
                    inserted: vec![nid(4)],
                },
            ],
        };
        let inv = cs.invert();
        assert_eq!(inv.changes.len(), 2);
        match &inv.changes[0] {
            DocChange::TreeSpliced {
                removed, inserted, ..
            } => {
                assert_eq!(removed, &vec![nid(4)]);
                assert_eq!(inserted, &vec![nid(3)]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(inv.invert(), cs);
    }

    #[test]
    fn is_text_only_is_all_text_changed_and_nonempty() {
        let text = ChangeSet {
            before_revision: 1,
            after_revision: 2,
            changes: vec![DocChange::text(nid(1), 1, 2, 0..0, "", "x")],
        };
        assert!(text.is_text_only());
        assert!(text.invert().is_text_only());
        assert!(!ChangeSet::empty(1).is_text_only());
        assert!(!ChangeSet::document_replaced(1).is_text_only());
        let mixed = ChangeSet {
            before_revision: 1,
            after_revision: 2,
            changes: vec![
                DocChange::text(nid(1), 1, 2, 0..0, "", "x"),
                DocChange::attrs(
                    nid(1),
                    BlockKind::Paragraph,
                    BlockKind::Paragraph,
                    NodeExtra::None,
                    NodeExtra::None,
                ),
            ],
        };
        assert!(!mixed.is_text_only());
        assert!(!mixed.is_structural());
    }
}
