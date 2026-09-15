use super::Document;
use super::arena::NodeId;

impl Document {
    pub(crate) fn reclaim(&mut self, protected: &[NodeId]) {
        let closure = self.arena.frozen_closure(protected);
        let retired = self.arena.retire_unreferenced(&closure);
        if retired.is_empty() {
            return;
        }
        for index in retired {
            self.texts.clear_slot(index);
            self.table_alignment_overflow
                .retain(|k, _| k.index != index);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::document::{editor_options, load_markdown};
    use std::sync::Arc;

    #[test]
    fn reclaim_clears_the_side_tables_of_retired_slots() {
        let mut doc = load_markdown("a\n\nb\n", editor_options());
        let root = doc.root;
        let paras: Vec<_> = doc.arena.children(root).collect();
        doc.arena.detach(paras[1]);
        doc.arena.tombstone(paras[1]);

        doc.reclaim(&[]);
        assert_eq!(doc.arena.free_list_len(), 1);
        assert!(
            doc.arena.frozen_node(paras[1]).is_none(),
            "the frozen record of a retired slot is deleted"
        );

        let reused = doc.alloc_container(crate::block::BlockKind::BlockQuote);
        assert_eq!(reused.index, paras[1].index);
        assert!(reused.generation.get() > paras[1].generation.get());
        assert_eq!(
            doc.texts.index_len() as u32,
            reused.index + 1,
            "reuse must not grow the texts slot table"
        );
    }

    #[test]
    fn a_history_referenced_tombstone_stays_out_of_the_free_list() {
        let mut doc = load_markdown("a\n\nb\n", editor_options());
        let root = doc.root;
        let paras: Vec<_> = doc.arena.children(root).collect();
        doc.arena.detach(paras[1]);
        doc.arena.tombstone(paras[1]);

        doc.arena.retain(paras[1]);
        doc.reclaim(&[paras[1]]);
        assert_eq!(
            doc.arena.free_list_len(),
            0,
            "a slot referenced by the history must not be recycled"
        );

        doc.arena.release(paras[1]);
        doc.reclaim(&[]);
        assert_eq!(doc.arena.free_list_len(), 1);
    }

    #[test]
    fn the_frozen_chain_of_a_referenced_dead_parent_is_protected() {
        let mut doc = load_markdown("> q1\n\n> q2\n", editor_options());
        let root = doc.root;
        let quotes: Vec<_> = doc.arena.children(root).collect();
        let children: Vec<_> = doc.arena.children(quotes[0]).collect();
        assert_eq!(children.len(), 1, "one paragraph inside the first quote");

        doc.arena.detach(quotes[0]);
        doc.arena.tombstone(quotes[0]);
        doc.arena.tombstone(children[0]);

        doc.arena.retain(quotes[0]);
        doc.reclaim(&[quotes[0]]);
        assert_eq!(doc.arena.free_list_len(), 0);

        doc.arena.release(quotes[0]);
        doc.reclaim(&[]);
        assert_eq!(doc.arena.free_list_len(), 2);
    }

    #[test]
    fn reclaim_drops_the_alignment_overflow_of_a_retired_table() {
        let mut doc = load_markdown("a\n", editor_options());
        let root = doc.root;
        let table = doc.arena.children(root).next().unwrap();
        doc.table_alignment_overflow
            .insert(table, Arc::from(vec![1u8, 2u8]));

        doc.arena.detach(table);
        doc.arena.tombstone(table);
        doc.reclaim(&[]);
        assert_eq!(doc.arena.free_list_len(), 1);
        assert!(
            doc.table_alignment_overflow.is_empty(),
            "the overflow entry of a retired slot is dropped"
        );
    }
}
