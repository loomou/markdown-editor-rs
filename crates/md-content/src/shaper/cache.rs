use super::artifact::ShapeArtifact;
use md_layout::shaper::ShapeIdentity;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const MAX_ENTRIES: usize = 4096;

const LOW_WATER: usize = MAX_ENTRIES - MAX_ENTRIES / 4;

const KEEP_FRAMES: u64 = 2;

const MAX_SHAPE_BYTES: usize = 48 * 1024 * 1024;
const BYTE_LOW_WATER: usize = MAX_SHAPE_BYTES - MAX_SHAPE_BYTES / 4;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct Key {
    pub(super) ident: ShapeIdentity,
    pub(super) width_q: i64,
    pub(super) style_fp: u64,
    pub(super) media_q: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub measure_calls: u64,
    pub shape_calls: u64,

    pub total_measure_calls: u64,
    pub total_shape_calls: u64,

    pub artifact_reuses: u64,

    pub probe_calls: u64,
}

struct Entry {
    art: Rc<ShapeArtifact>,

    frame: u64,
    bytes: usize,
}

pub struct ShapeCache {
    memo: RefCell<HashMap<Key, Entry>>,
    env_fp: Cell<u64>,
    frame: Cell<u64>,
    bytes: Cell<usize>,
    stats: RefCell<CacheStats>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CacheStats {
    pub frames: u64,

    pub env_invalidations: u64,

    pub cross_frame_hits: u64,

    pub same_frame_hits: u64,

    pub misses: u64,

    pub evictions: u64,
}

impl ShapeCache {
    pub fn new() -> Rc<Self> {
        Rc::new(ShapeCache {
            memo: RefCell::new(HashMap::new()),
            env_fp: Cell::new(0),
            frame: Cell::new(0),
            bytes: Cell::new(0),
            stats: RefCell::new(CacheStats::default()),
        })
    }

    pub fn begin_frame(&self, env_fp: u64) {
        self.frame.set(self.frame.get() + 1);
        let mut s = self.stats.borrow_mut();
        s.frames += 1;
        if self.env_fp.get() != env_fp {
            self.env_fp.set(env_fp);
            let mut memo = self.memo.borrow_mut();
            memo.clear();
            memo.shrink_to_fit();
            self.bytes.set(0);
            s.env_invalidations += 1;
        }
    }

    pub fn end_frame(&self) {
        let current = self.frame.get();
        let mut memo = self.memo.borrow_mut();
        let mut bytes = self.bytes.get();
        let evicted = evict_stale(&mut memo, &mut bytes, current)
            + evict_over_bytes(&mut memo, &mut bytes, current);
        self.bytes.set(bytes);
        if evicted > 0 {
            memo.shrink_to_fit();
            self.stats.borrow_mut().evictions += evicted as u64;
        }
    }

    pub fn entry_count(&self) -> usize {
        self.memo.borrow().len()
    }

    pub fn byte_count(&self) -> usize {
        self.bytes.get()
    }

    pub fn stats(&self) -> CacheStats {
        *self.stats.borrow()
    }

    pub(super) fn get(&self, key: &Key) -> Option<Rc<ShapeArtifact>> {
        let (art, cross) = {
            let mut memo = self.memo.borrow_mut();
            let Some(e) = memo.get_mut(key) else {
                self.stats.borrow_mut().misses += 1;
                return None;
            };
            let cross = e.frame < self.frame.get();

            e.frame = self.frame.get();
            (Rc::clone(&e.art), cross)
        };
        let mut s = self.stats.borrow_mut();
        if cross {
            s.cross_frame_hits += 1;
        } else {
            s.same_frame_hits += 1;
        }
        Some(art)
    }

    pub(super) fn insert(&self, key: Key, art: Rc<ShapeArtifact>) {
        let weight = art.weight_bytes();
        self.insert_weighted(key, art, weight);
    }

    fn insert_weighted(&self, key: Key, art: Rc<ShapeArtifact>, weight: usize) {
        let mut memo = self.memo.borrow_mut();
        let mut bytes = self.bytes.get();
        if let Some(old) = memo.insert(
            key,
            Entry {
                art,
                frame: self.frame.get(),
                bytes: weight,
            },
        ) {
            bytes = bytes.saturating_sub(old.bytes);
        }
        bytes = bytes.saturating_add(weight);
        let mut evicted = 0usize;
        if memo.len() > MAX_ENTRIES {
            evicted += evict_to_watermark(&mut memo, &mut bytes);
        }
        if bytes > MAX_SHAPE_BYTES {
            evicted += evict_over_bytes(&mut memo, &mut bytes, self.frame.get());
        }
        self.bytes.set(bytes);
        if evicted > 0 {
            memo.shrink_to_fit();
            self.stats.borrow_mut().evictions += evicted as u64;
        }
    }

    #[cfg(test)]
    pub(super) fn insert_with_bytes(&self, key: Key, art: Rc<ShapeArtifact>, bytes: usize) {
        self.insert_weighted(key, art, bytes.max(1));
    }
}

fn evict_to_watermark(memo: &mut HashMap<Key, Entry>, bytes: &mut usize) -> usize {
    let evict = memo.len().saturating_sub(LOW_WATER);
    if evict == 0 {
        return 0;
    }
    let mut by_frame: Vec<(u64, Key)> = memo
        .iter()
        .map(|(key, entry)| (entry.frame, key.clone()))
        .collect();
    by_frame.select_nth_unstable_by_key(evict - 1, |v| v.0);
    for (_, key) in by_frame.into_iter().take(evict) {
        if let Some(entry) = memo.remove(&key) {
            *bytes = bytes.saturating_sub(entry.bytes);
        }
    }
    evict
}

fn evict_stale(memo: &mut HashMap<Key, Entry>, bytes: &mut usize, current: u64) -> usize {
    let mut n = 0usize;
    memo.retain(|_, entry| {
        if entry.frame.saturating_add(KEEP_FRAMES) >= current {
            true
        } else {
            *bytes = bytes.saturating_sub(entry.bytes);
            n += 1;
            false
        }
    });
    n
}

fn evict_over_bytes(memo: &mut HashMap<Key, Entry>, bytes: &mut usize, current: u64) -> usize {
    if *bytes <= MAX_SHAPE_BYTES {
        return 0;
    }
    let mut by_frame: Vec<(u64, Key)> = memo
        .iter()
        .filter(|(_, entry)| entry.frame < current)
        .map(|(key, entry)| (entry.frame, key.clone()))
        .collect();
    by_frame.sort_unstable_by_key(|v| v.0);
    let mut n = 0usize;
    for (_, key) in by_frame {
        if *bytes <= BYTE_LOW_WATER {
            break;
        }
        if let Some(entry) = memo.remove(&key) {
            *bytes = bytes.saturating_sub(entry.bytes);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::{Key, ShapeCache};
    use crate::shaper::artifact::ShapeArtifact;
    use md_layout::shaper::ShapeIdentity;
    use std::rc::Rc;

    fn key() -> Key {
        Key {
            ident: ShapeIdentity::default(),
            width_q: 0,
            style_fp: 0,
            media_q: 0,
        }
    }

    fn key_with_index(index: u32) -> Key {
        Key {
            ident: ShapeIdentity {
                index,
                ..ShapeIdentity::default()
            },
            width_q: 0,
            style_fp: 0,
            media_q: 0,
        }
    }

    fn art() -> Rc<ShapeArtifact> {
        Rc::new(ShapeArtifact::plain(vec![], 1, 0.0, 0.0, 0.0))
    }

    #[test]
    fn misses_counter_covers_cold_lookups_and_env_invalidation() {
        let cache = ShapeCache::new();

        cache.begin_frame(0);

        assert!(cache.get(&key()).is_none());
        assert_eq!(
            cache.stats().misses,
            1,
            "a first lookup of a new key is a miss"
        );

        cache.insert(key(), art());
        assert!(cache.get(&key()).is_some());
        let s = cache.stats();
        assert_eq!(s.misses, 1, "a hit must not count as a miss");
        assert_eq!(s.same_frame_hits, 1);

        cache.begin_frame(0);
        assert!(cache.get(&key()).is_some());
        let s = cache.stats();
        assert_eq!(s.cross_frame_hits, 1, "a cross-frame lookup is still a hit");
        assert_eq!(s.misses, 1);

        cache.begin_frame(1);
        assert!(cache.get(&key()).is_none());
        let s = cache.stats();
        assert_eq!(s.env_invalidations, 1);
        assert_eq!(
            s.misses, 2,
            "a cold lookup after env invalidation also counts as a miss"
        );
    }

    #[test]
    fn full_cache_drops_to_watermark_in_one_batch() {
        let cache = ShapeCache::new();
        cache.begin_frame(0);
        cache.insert(key_with_index(0), art());
        cache.begin_frame(0);
        for i in 1..super::MAX_ENTRIES as u32 {
            cache.insert(key_with_index(i), art());
        }

        cache.begin_frame(0);
        cache.insert(key_with_index(super::MAX_ENTRIES as u32), art());
        assert_eq!(
            cache.entry_count(),
            super::LOW_WATER,
            "overflow did not drop the cache back to the low watermark"
        );
        assert_eq!(
            cache.stats().evictions as usize,
            super::MAX_ENTRIES + 1 - super::LOW_WATER,
            "the eviction counter does not match the number actually evicted"
        );
        assert!(
            cache.get(&key_with_index(0)).is_none(),
            "the oldest entry was not evicted"
        );
        assert!(
            cache
                .get(&key_with_index(super::MAX_ENTRIES as u32))
                .is_some(),
            "the newest entry was swept out with the batch"
        );
    }

    #[test]
    fn eviction_prefers_cold_entries_over_hot_ones() {
        let cache = ShapeCache::new();
        cache.begin_frame(0);
        let hot = key_with_index(0);
        let cold = key_with_index(1);
        cache.insert(cold.clone(), art());
        cache.begin_frame(0);
        cache.insert(hot.clone(), art());
        for i in 2..super::MAX_ENTRIES as u32 {
            cache.insert(key_with_index(i), art());
        }
        cache.begin_frame(0);
        assert!(cache.get(&hot).is_some());
        cache.insert(key_with_index(super::MAX_ENTRIES as u32), art());

        assert!(
            cache.get(&cold).is_none(),
            "a cold entry that was never hit should be evicted first"
        );
        assert!(
            cache.get(&hot).is_some(),
            "the repeatedly hit hot entry was swept out with the batch"
        );
    }

    #[test]
    fn stale_entries_drop_after_keep_frames() {
        let cache = ShapeCache::new();
        cache.begin_frame(0);
        cache.insert(key(), art());
        cache.end_frame();
        assert_eq!(cache.entry_count(), 1);
        for _ in 0..super::KEEP_FRAMES {
            cache.begin_frame(0);
            cache.end_frame();
            assert_eq!(cache.entry_count(), 1);
        }
        cache.begin_frame(0);
        cache.end_frame();
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn touched_entries_survive_keep_frames() {
        let cache = ShapeCache::new();
        cache.begin_frame(0);
        cache.insert(key(), art());
        cache.end_frame();
        for _ in 0..8 {
            cache.begin_frame(0);
            assert!(cache.get(&key()).is_some());
            cache.end_frame();
        }
        assert_eq!(cache.entry_count(), 1);
    }

    #[test]
    fn scrolling_unique_keys_stays_near_the_keep_window() {
        let cache = ShapeCache::new();
        let per_frame = 8u32;
        let frames = 80u32;
        for f in 0..frames {
            cache.begin_frame(0);
            for j in 0..per_frame {
                cache.insert(key_with_index(f * per_frame + j), art());
            }
            cache.end_frame();
        }
        let n = cache.entry_count();
        assert!(n <= super::MAX_ENTRIES);
        assert!(
            n <= (per_frame as usize) * (super::KEEP_FRAMES as usize + 1),
            "shape cache grew with scroll distance: {n}"
        );
        assert_eq!(n, (per_frame as usize) * (super::KEEP_FRAMES as usize + 1));
    }

    #[test]
    fn byte_budget_evicts_older_frames_to_low_water() {
        let cache = ShapeCache::new();
        let chunk = 10 * 1024 * 1024;
        cache.begin_frame(0);
        for i in 0..5 {
            cache.insert_with_bytes(key_with_index(i), art(), chunk);
        }
        assert!(cache.byte_count() > super::MAX_SHAPE_BYTES);
        cache.end_frame();
        cache.begin_frame(0);
        cache.end_frame();
        assert!(cache.byte_count() <= super::MAX_SHAPE_BYTES);
        assert!(cache.byte_count() <= super::BYTE_LOW_WATER);
        assert!(cache.entry_count() < 5);
        assert!(cache.stats().evictions > 0);
    }
}
