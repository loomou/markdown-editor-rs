use gpui::RenderImage;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[derive(Default)]
pub struct AllocationLedger {
    entries: HashMap<usize, LedgerEntry>,
    bytes: usize,
}

struct LedgerEntry {
    weak: Weak<RenderImage>,
    bytes: usize,
}

impl AllocationLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn track(&mut self, image: &Arc<RenderImage>, bytes: usize) {
        let id = Arc::as_ptr(image) as *const () as usize;
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry.weak.strong_count() > 0 {
                if bytes > entry.bytes {
                    self.bytes = self.bytes.saturating_add(bytes - entry.bytes);
                    entry.bytes = bytes;
                }
                return;
            }
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            self.entries.remove(&id);
        }
        self.entries.insert(
            id,
            LedgerEntry {
                weak: Arc::downgrade(image),
                bytes,
            },
        );
        self.bytes = self.bytes.saturating_add(bytes);
    }

    pub fn settle(&mut self) -> usize {
        let mut freed: usize = 0;
        self.entries.retain(|_, entry| {
            if entry.weak.strong_count() > 0 {
                return true;
            }
            freed = freed.saturating_add(entry.bytes);
            false
        });
        self.bytes = self.bytes.saturating_sub(freed);
        self.bytes
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::AllocationLedger;
    use gpui::RenderImage;
    use image::RgbaImage;
    use smallvec::smallvec;
    use std::sync::Arc;

    fn image(w: u32, h: u32) -> Arc<RenderImage> {
        Arc::new(RenderImage::new(smallvec![image::Frame::new(
            RgbaImage::new(w, h)
        )]))
    }

    #[test]
    fn the_same_allocation_is_counted_once() {
        let mut ledger = AllocationLedger::new();
        let shared = image(4, 4);
        ledger.track(&shared, 64);
        ledger.track(&shared, 64);
        ledger.track(&shared, 0);
        assert_eq!(ledger.bytes(), 64);
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn a_live_allocation_is_not_written_off_when_its_tracker_goes_away() {
        let mut ledger = AllocationLedger::new();
        let held = image(4, 4);
        let bare = Arc::clone(&held);
        ledger.track(&held, 64);
        drop(held);
        assert_eq!(
            ledger.settle(),
            64,
            "a bare reference still holds the pixels"
        );
        drop(bare);
        assert_eq!(ledger.settle(), 0, "the last reference settles the entry");
        assert!(ledger.is_empty());
    }

    #[test]
    fn a_reused_address_does_not_inherit_the_old_entry() {
        let mut ledger = AllocationLedger::new();
        let first = image(4, 4);
        ledger.track(&first, 64);
        drop(first);
        let second = image(8, 8);
        ledger.track(&second, 256);
        assert_eq!(ledger.settle(), 256);
        assert_eq!(ledger.len(), 1);
    }
}
