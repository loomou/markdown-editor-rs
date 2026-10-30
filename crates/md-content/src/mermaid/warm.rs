use super::{MermaidCache, MermaidKey, MermaidRasterKey, Slot};
use crate::media::{COOL_MIN_GAIN_PERCENT, MediaUsage, WarmImage};
use crate::pixels::ReadyImage;
use gpui::App;
use std::time::Instant;

impl MermaidCache {
    pub(super) fn note_working_set(&mut self, now: Instant) {
        let keys: Vec<MermaidRasterKey> = self.map.keys().copied().collect();
        for key in keys {
            if self.hot.contains(&key) {
                self.left_screen.remove(&key);
            } else {
                self.left_screen.entry(key).or_insert(now);
            }
        }
        self.left_screen.retain(|key, _| self.map.contains_key(key));
        self.cool_refused.retain(|key| self.map.contains_key(key));
    }

    /// Diagrams that have been off screen long enough to be worth compressing.
    /// A diagram is the most expensive thing in the cache to rebuild — layout,
    /// render, encode, decode — so it is also the one that gains most from
    /// keeping its content instead of its width.
    pub fn coolable_at(&self, now: Instant, pressured: bool) -> Vec<MermaidKey> {
        if !crate::media::cooling_enabled() {
            return Vec::new();
        }
        let grace = if pressured {
            std::time::Duration::ZERO
        } else {
            crate::media::cool_grace()
        };
        let mut out = Vec::new();
        for (raster, slot) in &self.map {
            if !matches!(slot, Slot::Ready(_)) {
                continue;
            }
            if self.hot.contains(raster)
                || self.cooling.contains(raster)
                || self.cool_refused.contains(raster)
            {
                continue;
            }
            let Some(since) = self.left_screen.get(raster) else {
                continue;
            };
            if now.duration_since(*since) < grace {
                continue;
            }
            if let Some(key) = self.key_for_raster(*raster) {
                out.push(key);
            }
        }
        out
    }

    fn key_for_raster(&self, raster: MermaidRasterKey) -> Option<MermaidKey> {
        self.latest
            .iter()
            .find(|(_, current)| **current == raster)
            .map(|((block, generation), _)| MermaidKey {
                block: *block,
                generation: *generation,
                raster,
            })
    }

    pub fn begin_cool(&mut self, key: &MermaidKey) -> Option<(MermaidKey, ReadyImage)> {
        let Slot::Ready(image) = self.map.get(&key.raster)? else {
            return None;
        };
        if self.hot.contains(&key.raster) || self.cooling.contains(&key.raster) {
            return None;
        }
        self.cooling.insert(key.raster);
        Some((*key, image.clone()))
    }

    pub fn cancel_cool(&mut self, key: &MermaidKey) {
        self.cooling.remove(&key.raster);
    }

    pub fn finish_cool(&mut self, key: &MermaidKey, warm: WarmImage, cx: &mut App) -> bool {
        self.finish_cool_inner(key, warm, Some(cx))
    }

    pub(super) fn finish_cool_inner(
        &mut self,
        key: &MermaidKey,
        warm: WarmImage,
        cx: Option<&mut App>,
    ) -> bool {
        if !self.cooling.remove(&key.raster) {
            return false;
        }
        let ready = match self.map.get(&key.raster) {
            Some(Slot::Ready(image)) if !self.hot.contains(&key.raster) => image.clone(),
            _ => {
                self.cool_discarded = self.cool_discarded.saturating_add(1);
                return false;
            }
        };
        if warm.resident_bytes().saturating_mul(100)
            > ready.bytes.saturating_mul(COOL_MIN_GAIN_PERCENT)
        {
            self.cool_refused.insert(key.raster);
            return false;
        }
        self.bytes = self.bytes.saturating_sub(ready.bytes);
        self.warm_bytes = self.warm_bytes.saturating_add(warm.resident_bytes());
        self.map.insert(key.raster, Slot::Warm(warm));
        if let Some(cx) = cx {
            MediaUsage::release_ready(cx, ready);
        }
        true
    }

    pub fn begin_thaw(&mut self, key: &MermaidKey) -> Option<WarmImage> {
        let Slot::Warm(warm) = self.map.get(&key.raster)? else {
            return None;
        };
        let warm = warm.clone();
        self.map.insert(key.raster, Slot::Thawing(warm.clone()));
        Some(warm)
    }

    pub fn cancel_thaw(&mut self, key: &MermaidKey) {
        let warm = match self.map.get(&key.raster) {
            Some(Slot::Thawing(warm)) => warm.clone(),
            _ => return,
        };
        self.map.insert(key.raster, Slot::Warm(warm));
    }

    pub fn finish_thaw(&mut self, key: &MermaidKey, image: Option<ReadyImage>) -> bool {
        let warm_bytes = match self.map.get(&key.raster) {
            Some(Slot::Thawing(warm)) => warm.resident_bytes(),
            _ => return false,
        };
        self.warm_bytes = self.warm_bytes.saturating_sub(warm_bytes);
        match image {
            Some(image) => {
                self.bytes = self.bytes.saturating_add(image.bytes);
                self.map.insert(key.raster, Slot::Ready(image));
                self.touch(key.raster);
                self.left_screen.remove(&key.raster);
                true
            }
            None => {
                self.map.remove(&key.raster);
                self.lru.retain(|candidate| *candidate != key.raster);
                self.latest.retain(|_, current| *current != key.raster);
                false
            }
        }
    }

    pub fn warm_account(&self) -> (usize, usize) {
        let entries = self
            .map
            .values()
            .filter(|slot| matches!(slot, Slot::Warm(_) | Slot::Thawing(_)))
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
