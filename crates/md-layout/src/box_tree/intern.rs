use md_core::document::LeafSnapshot;
use md_core::inline::InlineRun;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct BoxIntern {
    entries: Vec<Option<Arc<LeafSnapshot>>>,
    free: Vec<u32>,
    live: usize,
}

impl BoxIntern {
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn push(&mut self, snapshot: Option<Arc<LeafSnapshot>>) -> Option<u32> {
        let snapshot = snapshot?;
        if snapshot.display.is_empty() && snapshot.runs.is_empty() {
            return None;
        }
        let id = match self.free.pop() {
            Some(id) => {
                self.entries[id as usize] = Some(snapshot);
                id
            }
            None => {
                let id = u32::try_from(self.entries.len()).expect("BoxIntern exhausted");
                self.entries.push(Some(snapshot));
                id
            }
        };
        self.live += 1;
        Some(id)
    }

    pub fn replace(&mut self, id: u32, snapshot: Option<Arc<LeafSnapshot>>) {
        let snapshot =
            snapshot.filter(|snapshot| !snapshot.display.is_empty() || !snapshot.runs.is_empty());
        let slot = self
            .entries
            .get_mut(id as usize)
            .expect("BoxNode has an unknown text id");
        match (slot.is_some(), snapshot.is_some()) {
            (true, false) => {
                self.live -= 1;
                self.free.push(id);
            }
            (false, true) => {
                self.live += 1;
                self.free.retain(|free| *free != id);
            }
            _ => {}
        }
        *slot = snapshot;
    }

    pub fn release(&mut self, id: Option<u32>) {
        if let Some(id) = id
            && let Some(slot) = self.entries.get_mut(id as usize)
            && slot.take().is_some()
        {
            self.live -= 1;
            self.free.push(id);
        }
    }

    pub fn text(&self, id: Option<u32>) -> &str {
        id.and_then(|id| self.entries.get(id as usize))
            .and_then(Option::as_ref)
            .map(|snapshot| snapshot.display.as_str())
            .unwrap_or("")
    }

    pub fn runs(&self, id: Option<u32>) -> &[InlineRun] {
        id.and_then(|id| self.entries.get(id as usize))
            .and_then(Option::as_ref)
            .map(|snapshot| snapshot.runs.as_slice())
            .unwrap_or(&[])
    }
}

pub(crate) fn owned_snapshot(display: String, runs: Vec<InlineRun>) -> Option<Arc<LeafSnapshot>> {
    if display.is_empty() && runs.is_empty() {
        None
    } else {
        Some(Arc::new(LeafSnapshot { display, runs }))
    }
}

#[cfg(test)]
mod tests {
    use super::{BoxIntern, owned_snapshot};

    #[test]
    fn released_slots_are_reused_without_changing_live_ids() {
        let mut intern = BoxIntern::default();
        let first = intern
            .push(owned_snapshot("first".into(), Vec::new()))
            .expect("first");
        let second = intern
            .push(owned_snapshot("second".into(), Vec::new()))
            .expect("second");
        intern.release(Some(first));
        let reused = intern
            .push(owned_snapshot("third".into(), Vec::new()))
            .expect("third");

        assert_eq!(reused, first);
        assert_eq!(intern.text(Some(second)), "second");
        assert_eq!(intern.text(Some(reused)), "third");
        assert_eq!(intern.len(), 2);
    }
}
