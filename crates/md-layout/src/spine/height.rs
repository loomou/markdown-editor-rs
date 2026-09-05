use super::FlowSpine;
use super::item::{FlowItem, FlowItemId};
use crate::flow::HeightState;

impl FlowSpine {
    pub fn set_height(&mut self, id: FlowItemId, height: HeightState) {
        let Some(pos) = self.location(id) else {
            return;
        };
        let old = self.items[pos];
        let was_fresh_content_exact =
            old.is_content() && old.height.is_exact() && old.height_epoch == self.layout_epoch;
        let was_noncontent_est = !old.is_content() && !old.height.is_exact();
        self.items[pos].height = height;
        self.items[pos].height_epoch = self.layout_epoch;
        let now_fresh_content_exact = old.is_content() && height.is_exact();
        let now_noncontent_est = !old.is_content() && !height.is_exact();
        if old.is_content() {
            if was_fresh_content_exact && !now_fresh_content_exact {
                self.exact_content_in_epoch = self.exact_content_in_epoch.saturating_sub(1);
            } else if !was_fresh_content_exact && now_fresh_content_exact {
                self.exact_content_in_epoch += 1;
            }
        } else if was_noncontent_est && !now_noncontent_est {
            self.noncontent_estimated = self.noncontent_estimated.saturating_sub(1);
        } else if !was_noncontent_est && now_noncontent_est {
            self.noncontent_estimated += 1;
        }
        self.estimated_count = self.noncontent_estimated
            + self
                .content_count
                .saturating_sub(self.exact_content_in_epoch);
        let delta = height.px() - old.height.px();
        if delta != 0.0 {
            self.fenwick.add(pos, delta);
        }
    }

    pub fn splice(&mut self, at: usize, delete: usize, insert: Vec<FlowItem>) {
        let inserted = insert.len();
        self.tally(at..at + delete, false);
        self.splice_no_index(at, delete, insert);
        self.tally(at..at + inserted, true);
        self.splice_height_index(at, delete, inserted);
    }

    fn splice_no_index(&mut self, at: usize, delete: usize, insert: Vec<FlowItem>) {
        assert!(at + delete <= self.items.len());
        let removed: Vec<FlowItem> = self.items[at..at + delete].to_vec();
        for item in &removed {
            self.unmap(item);
        }
        self.items.splice(at..at + delete, insert);
        self.remap_from(at);
    }
}
