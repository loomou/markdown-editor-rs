use super::{CoolToken, DisplayKey, DisplaySlot, DisplayToken, ImageCache, WARM_MIN_GAIN_PERCENT};
use crate::media::{MediaUsage, WarmImage};
use gpui::App;
use std::time::Instant;

impl ImageCache {
    /// Stamp when each entry left the screen. An entry that is on screen has no
    /// stamp, so the grace restarts every time it comes back.
    pub(super) fn note_working_set(&mut self, now: Instant) {
        let keys: Vec<DisplayKey> = self.display.keys().cloned().collect();
        for key in keys {
            if self.display_visible.contains(&key) {
                self.left_screen.remove(&key);
            } else {
                self.left_screen.entry(key).or_insert(now);
            }
        }
        self.left_screen
            .retain(|key, _| self.display.contains_key(key));
        self.cool_refused
            .retain(|key| self.display.contains_key(key));
    }

    /// Entries that have been off-screen long enough to be worth compressing.
    /// The caller admits each one before it starts, so this is a candidate list
    /// rather than a decision.
    pub fn coolable_at(&self, now: Instant, pressured: bool) -> Vec<DisplayKey> {
        if !crate::media::cooling_enabled() {
            return Vec::new();
        }
        let grace = if pressured {
            std::time::Duration::ZERO
        } else {
            crate::media::cool_grace()
        };
        let mut out = Vec::new();
        for (key, slot) in &self.display {
            if !matches!(slot, DisplaySlot::Ready(_)) {
                continue;
            }
            if self.display_visible.contains(key)
                || self.cooling.contains_key(key)
                || self.cool_refused.contains(key)
            {
                continue;
            }
            let Some(since) = self.left_screen.get(key) else {
                continue;
            };
            if now.duration_since(*since) >= grace {
                out.push(key.clone());
            }
        }
        out
    }

    /// Take the pixels of an off-screen entry out to be compressed. The cache
    /// keeps its own copy, so a frame that still holds the snapshot is not
    /// affected; the clone only keeps the lease charged until the job ends.
    pub fn begin_cool(&mut self, key: &DisplayKey) -> Option<(CoolToken, super::ReadyImage)> {
        let DisplaySlot::Ready(image) = self.display.get(key)? else {
            return None;
        };
        if self.display_visible.contains(key) || self.cooling.contains_key(key) {
            return None;
        }
        let seq = self.job_seq;
        self.job_seq = self.job_seq.wrapping_add(1);
        self.cooling.insert(key.clone(), seq);
        Some((
            CoolToken {
                key: key.clone(),
                seq,
            },
            image.clone(),
        ))
    }

    /// Install the compressed form, but only if the entry is still there, still
    /// off-screen, and the compression actually paid. Anything else discards
    /// the result and lets the entry stay hot: a re-visit during the job, or a
    /// payload that barely shrank, are both reasons not to swap.
    pub fn finish_cool(&mut self, token: CoolToken, warm: WarmImage, cx: &mut App) -> bool {
        match self.cooling.get(&token.key) {
            Some(seq) if *seq == token.seq => {}
            _ => return false,
        }
        self.cooling.remove(&token.key);
        let ready = match self.display.get(&token.key) {
            Some(DisplaySlot::Ready(image)) if !self.display_visible.contains(&token.key) => {
                image.clone()
            }
            _ => {
                // The entry came back on screen, or was dropped, while the
                // encode ran. The bytes spent are the cost of the grace being
                // shorter than the visit.
                self.cool_discarded = self.cool_discarded.saturating_add(1);
                return false;
            }
        };
        if warm.resident_bytes().saturating_mul(100)
            > ready.bytes.saturating_mul(WARM_MIN_GAIN_PERCENT)
        {
            self.cool_refused.insert(token.key);
            return false;
        }
        self.display_bytes = self.display_bytes.saturating_sub(ready.bytes);
        self.warm_bytes = self.warm_bytes.saturating_add(warm.resident_bytes());
        self.display
            .insert(token.key.clone(), DisplaySlot::Warm(warm));
        self.clear_display_projection(&token.key);
        MediaUsage::release_ready(cx, ready);
        true
    }

    /// Start inflating a warm entry. Nothing else may be started for this key
    /// while it runs, so the slot records the job instead of staying `Warm`.
    pub fn begin_thaw(&mut self, key: &DisplayKey) -> Option<(DisplayToken, WarmImage)> {
        let DisplaySlot::Warm(warm) = self.display.get(key)? else {
            return None;
        };
        let warm = warm.clone();
        let seq = self.job_seq;
        self.job_seq = self.job_seq.wrapping_add(1);
        self.display.insert(
            key.clone(),
            DisplaySlot::Thawing {
                seq,
                warm: warm.clone(),
            },
        );
        Some((
            DisplayToken {
                key: key.clone(),
                seq,
            },
            warm,
        ))
    }

    /// Abandon a cooling job that never got a budget. The entry stays hot and
    /// the next update may try again.
    pub fn cancel_cool(&mut self, token: CoolToken) {
        match self.cooling.get(&token.key) {
            Some(seq) if *seq == token.seq => {
                self.cooling.remove(&token.key);
            }
            _ => {}
        }
    }

    /// Put a warm entry back the way it was when its inflation could not be
    /// admitted, so a later frame can try again.
    pub fn cancel_thaw(&mut self, token: DisplayToken) {
        let warm = match self.display.get(&token.key) {
            Some(DisplaySlot::Thawing { seq, warm }) if *seq == token.seq => warm.clone(),
            _ => return,
        };
        self.display.insert(token.key, DisplaySlot::Warm(warm));
    }

    pub fn finish_thaw(
        &mut self,
        token: DisplayToken,
        result: Option<super::ReadyImage>,
        cx: &mut App,
    ) -> bool {
        match self.display.get(&token.key) {
            Some(DisplaySlot::Thawing { seq, .. }) if *seq == token.seq => {}
            _ => return false,
        }
        let warm_bytes = match self.display.remove(&token.key) {
            Some(DisplaySlot::Thawing { warm, .. }) => warm.resident_bytes(),
            _ => 0,
        };
        self.warm_bytes = self.warm_bytes.saturating_sub(warm_bytes);
        let key = token.key;
        match result {
            Some(img) => {
                self.display_bytes = self.display_bytes.saturating_add(img.bytes);
                self.display
                    .insert(key.clone(), DisplaySlot::Ready(img.clone()));
                self.record_display_ready(key.clone(), img);
                self.touch_display(key.clone());
                self.left_screen.remove(&key);
                let entries = self.budget.display_entries;
                let bytes = self.budget.display_bytes;
                self.evict_display(Some(cx), entries, bytes);
                true
            }
            // A payload that will not inflate says nothing about the source:
            // drop the warm entry and let the next frame rasterise it again.
            None => {
                self.display_lru.retain(|candidate| *candidate != key);
                self.clear_display_projection(&key);
                false
            }
        }
    }

    /// Bytes and entries the compressed form is holding, for the account.
    pub fn warm_account(&self) -> (usize, usize) {
        let entries = self
            .display
            .values()
            .filter(|slot| matches!(slot, DisplaySlot::Warm(_) | DisplaySlot::Thawing { .. }))
            .count();
        (self.warm_bytes, entries)
    }

    #[cfg(test)]
    pub fn warm_entry_count(&self) -> usize {
        self.warm_account().1
    }

    #[cfg(test)]
    pub fn warm_bytes(&self) -> usize {
        self.warm_bytes
    }
}
