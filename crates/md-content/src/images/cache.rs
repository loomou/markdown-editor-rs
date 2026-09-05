use super::raster::decoded_len;
use super::{
    DisplayKey, DisplaySlot, ImageCache, MAX_DISPLAY_BYTES, MAX_DISPLAY_ENTRIES, MAX_IN_FLIGHT,
    MAX_SOURCE_BYTES, SourceKey, SourceSlot, WARM_DISPLAY_EXTRA, WARM_SOURCE_IN_FLIGHT,
};
use crate::pixels::ReadyImage;
use gpui::{App, RenderImage};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

impl ImageCache {
    pub fn new() -> Self {
        ImageCache {
            source: HashMap::new(),
            source_lru: VecDeque::new(),
            source_visible: HashSet::new(),
            source_warm: HashSet::new(),
            source_bytes: 0,
            source_inflight: 0,
            display: HashMap::new(),
            display_lru: VecDeque::new(),
            display_visible: HashSet::new(),
            display_bytes: 0,
            display_inflight: 0,
            sizes_gen: 0,
            sizes: Rc::new(HashMap::new()),
            ready: Rc::new(HashMap::new()),
            failed_src: Rc::new(HashSet::new()),
        }
    }

    pub fn sizes_snapshot(&self) -> Rc<HashMap<SourceKey, (u32, u32)>> {
        Rc::clone(&self.sizes)
    }

    pub fn sizes_gen(&self) -> u64 {
        self.sizes_gen
    }

    pub fn source_contains(&self, dest: &str) -> bool {
        self.source.contains_key(dest)
    }

    #[cfg(test)]
    pub fn intrinsic(&self, dest: &str) -> Option<(u32, u32)> {
        self.sizes.get(dest).copied()
    }

    pub fn source_image(&self, dest: &str) -> Option<Arc<RenderImage>> {
        match self.source.get(dest) {
            Some(SourceSlot::Ready { image }) => Some(Arc::clone(image)),
            _ => None,
        }
    }

    pub fn begin_source(&mut self, dest: String) -> bool {
        let key = SourceKey::new(dest);
        if self.source.contains_key(&key) {
            return false;
        }
        let budget = if self.source_warm.contains(&key) {
            WARM_SOURCE_IN_FLIGHT
        } else {
            MAX_IN_FLIGHT
        };
        if self.source_inflight >= budget {
            return false;
        }
        self.source.insert(key, SourceSlot::InFlight);
        self.source_inflight += 1;
        true
    }

    pub fn finish_source(
        &mut self,
        dest: String,
        result: Result<(Arc<RenderImage>, u32, u32), crate::Error>,
        cx: &mut App,
    ) -> bool {
        self.finish_source_inner(dest, result, Some(cx), MAX_SOURCE_BYTES)
    }

    pub(super) fn finish_source_inner(
        &mut self,
        dest: String,
        result: Result<(Arc<RenderImage>, u32, u32), crate::Error>,
        cx: Option<&mut App>,
        source_budget: usize,
    ) -> bool {
        let key = SourceKey::new(dest);
        let Some(slot) = self.source.get(&key) else {
            return false;
        };
        if !matches!(slot, SourceSlot::InFlight) {
            return false;
        }
        self.source_inflight = self.source_inflight.saturating_sub(1);
        match result {
            Ok((image, width, height)) if width > 0 && height > 0 => {
                let n = decoded_len(&image);
                self.evict_source(n, source_budget, cx);
                if self.source_bytes.saturating_add(n) > source_budget {
                    self.source.insert(
                        key.clone(),
                        SourceSlot::OverBudget {
                            err: crate::Error::Image(md_i18n::Key::ImageOverBudget.into()),
                            wanted: n,
                        },
                    );
                    self.mark_source_failed(key);
                    return true;
                }
                self.source_bytes = self.source_bytes.saturating_add(n);
                self.source.insert(key.clone(), SourceSlot::Ready { image });
                self.record_source_size(key.clone(), width, height);
                self.touch_source(key);
                true
            }
            Ok(_) => {
                self.source.insert(
                    key.clone(),
                    SourceSlot::Failed(crate::Error::Image(md_i18n::Key::ImageEmpty.into())),
                );
                self.mark_source_failed(key);
                true
            }
            Err(err) => {
                self.source.insert(key.clone(), SourceSlot::Failed(err));

                self.mark_source_failed(key);
                true
            }
        }
    }

    pub fn display_contains(&self, key: &DisplayKey) -> bool {
        matches!(
            self.display.get(key),
            Some(DisplaySlot::Ready(_)) | Some(DisplaySlot::InFlight)
        )
    }

    #[cfg(test)]
    pub fn display_ready(&self, key: &DisplayKey) -> Option<ReadyImage> {
        match self.display.get(key) {
            Some(DisplaySlot::Ready(img)) => Some(img.clone()),
            _ => None,
        }
    }

    pub fn ready_snapshot(&self) -> Rc<HashMap<DisplayKey, ReadyImage>> {
        Rc::clone(&self.ready)
    }

    pub fn failed_sources(&self) -> Rc<HashSet<SourceKey>> {
        Rc::clone(&self.failed_src)
    }

    pub fn source_error(&self, dest: &str) -> Option<&crate::Error> {
        match self.source.get(dest) {
            Some(SourceSlot::Failed(err) | SourceSlot::OverBudget { err, .. }) => Some(err),
            _ => None,
        }
    }

    pub fn begin_display(&mut self, key: DisplayKey) -> bool {
        if matches!(
            self.display.get(&key),
            Some(DisplaySlot::Ready(_) | DisplaySlot::InFlight)
        ) {
            return false;
        }
        self.display.remove(&key);
        self.display_lru.retain(|candidate| candidate != &key);
        if self.display_inflight >= MAX_IN_FLIGHT {
            return false;
        }
        self.display.insert(key, DisplaySlot::InFlight);
        self.display_inflight += 1;
        true
    }

    pub fn finish_display(
        &mut self,
        key: DisplayKey,
        result: Result<ReadyImage, crate::Error>,
        cx: &mut App,
    ) -> bool {
        self.finish_display_inner(key, result, Some(cx))
    }

    pub(super) fn finish_display_inner(
        &mut self,
        key: DisplayKey,
        result: Result<ReadyImage, crate::Error>,
        cx: Option<&mut App>,
    ) -> bool {
        let Some(slot) = self.display.get(&key) else {
            return false;
        };
        if !matches!(slot, DisplaySlot::InFlight) {
            return false;
        }
        self.display_inflight = self.display_inflight.saturating_sub(1);
        match result {
            Ok(img) => {
                self.display_bytes = self.display_bytes.saturating_add(img.bytes);
                self.display
                    .insert(key.clone(), DisplaySlot::Ready(img.clone()));
                self.record_display_ready(key.clone(), img);
                self.touch_display(key);
                self.evict_display(cx, MAX_DISPLAY_ENTRIES, MAX_DISPLAY_BYTES);
                true
            }
            Err(_err) => {
                self.display.remove(&key);
                self.display_lru.retain(|candidate| candidate != &key);
                false
            }
        }
    }

    pub fn set_working_set(
        &mut self,
        hot: impl IntoIterator<Item = DisplayKey>,
        hot_sources: impl IntoIterator<Item = String>,
        warm_sources: impl IntoIterator<Item = String>,
        cx: &mut App,
    ) {
        self.set_working_set_inner(hot, hot_sources, warm_sources, Some(cx), MAX_SOURCE_BYTES);
    }

    pub(super) fn set_working_set_inner(
        &mut self,
        hot: impl IntoIterator<Item = DisplayKey>,
        hot_sources: impl IntoIterator<Item = String>,
        warm_sources: impl IntoIterator<Item = String>,
        mut cx: Option<&mut App>,
        source_budget: usize,
    ) {
        self.display_visible.clear();
        self.display_visible.extend(hot);
        self.source_visible.clear();
        self.source_visible.extend(
            self.display_visible
                .iter()
                .map(|key| SourceKey::new(key.dest.clone())),
        );
        self.source_visible
            .extend(hot_sources.into_iter().map(SourceKey::new));
        self.source_warm.clear();
        self.source_warm
            .extend(warm_sources.into_iter().map(SourceKey::new));

        self.source_warm
            .retain(|key| !self.source_visible.contains(key));
        let ready: Vec<DisplayKey> = self
            .display_visible
            .iter()
            .filter(|key| matches!(self.display.get(*key), Some(DisplaySlot::Ready(_))))
            .cloned()
            .collect();
        for key in ready {
            self.touch_display(key);
        }

        let mut pinned = 0usize;
        let ready_sources: Vec<SourceKey> = self
            .source_visible
            .iter()
            .filter_map(|key| match self.source.get(key) {
                Some(SourceSlot::Ready { image }) => {
                    pinned = pinned.saturating_add(decoded_len(image));
                    Some(key.clone())
                }
                _ => None,
            })
            .collect();
        for key in ready_sources {
            self.touch_source(key);
        }
        let retry: Vec<SourceKey> = self
            .source
            .iter()
            .filter_map(|(key, slot)| {
                let SourceSlot::OverBudget { wanted, .. } = slot else {
                    return None;
                };

                let working = self.source_visible.contains(key) || self.source_warm.contains(key);
                (!working || pinned.saturating_add(*wanted) <= source_budget).then(|| key.clone())
            })
            .collect();
        for key in retry {
            self.source.remove(&key);
            self.clear_source_projections(&key);
        }
        self.evict_source(0, source_budget, cx.as_deref_mut());

        let warm_entries = self
            .display_visible
            .len()
            .saturating_add(self.warm_display_count())
            .saturating_add(WARM_DISPLAY_EXTRA)
            .min(MAX_DISPLAY_ENTRIES);
        self.evict_display(cx, warm_entries, MAX_DISPLAY_BYTES);
    }

    pub fn clear(&mut self, cx: &mut App) {
        for slot in self.source.values() {
            if let SourceSlot::Ready { image, .. } = slot {
                cx.drop_image(image.clone(), None);
            }
        }
        for slot in self.display.values() {
            if let DisplaySlot::Ready(img) = slot
                && !img.shared_with_source
            {
                cx.drop_image(img.image.clone(), None);
            }
        }
        self.source.clear();
        self.source_lru.clear();
        self.source_visible.clear();
        self.source_warm.clear();
        self.source_bytes = 0;
        self.source_inflight = 0;
        self.display.clear();
        self.display_lru.clear();
        self.display_visible.clear();
        self.display_bytes = 0;
        self.display_inflight = 0;
        self.sizes_gen = 0;
        self.sizes = Rc::new(HashMap::new());
        self.ready = Rc::new(HashMap::new());
        self.failed_src = Rc::new(HashSet::new());
    }

    fn touch_display(&mut self, key: DisplayKey) {
        self.display_lru.retain(|k| *k != key);
        self.display_lru.push_back(key);
    }

    fn touch_source(&mut self, key: SourceKey) {
        self.source_lru.retain(|candidate| *candidate != key);
        self.source_lru.push_back(key);
    }

    fn mark_source_failed(&mut self, key: SourceKey) {
        Rc::make_mut(&mut self.failed_src).insert(key);
        self.sizes_gen = self.sizes_gen.wrapping_add(1);
    }

    fn record_source_size(&mut self, key: SourceKey, width: u32, height: u32) {
        Rc::make_mut(&mut self.sizes).insert(key, (width, height));
        self.sizes_gen = self.sizes_gen.wrapping_add(1);
    }

    fn clear_source_projections(&mut self, key: &SourceKey) {
        Rc::make_mut(&mut self.sizes).remove(key);
        Rc::make_mut(&mut self.failed_src).remove(key);
        self.sizes_gen = self.sizes_gen.wrapping_add(1);
    }

    fn record_display_ready(&mut self, key: DisplayKey, image: ReadyImage) {
        Rc::make_mut(&mut self.ready).insert(key, image);
    }

    fn clear_display_projection(&mut self, key: &DisplayKey) {
        Rc::make_mut(&mut self.ready).remove(key);
    }

    fn pick_source_victim(&self) -> Option<usize> {
        let evictable = |key: &SourceKey| {
            !self.source_visible.contains(key)
                && matches!(self.source.get(key), Some(SourceSlot::Ready { .. }))
        };
        self.source_lru
            .iter()
            .position(|key| evictable(key) && !self.source_warm.contains(key))
            .or_else(|| self.source_lru.iter().position(evictable))
    }

    fn evict_source(&mut self, incoming: usize, source_budget: usize, mut cx: Option<&mut App>) {
        while self.source_bytes.saturating_add(incoming) > source_budget {
            let Some(idx) = self.pick_source_victim() else {
                break;
            };
            let victim = self.source_lru.remove(idx).expect("source lru");
            self.remove_source(&victim, cx.as_deref_mut());
        }
    }

    fn remove_source(&mut self, key: &SourceKey, mut cx: Option<&mut App>) {
        if let Some(SourceSlot::Ready { image }) = self.source.remove(key) {
            self.source_bytes = self.source_bytes.saturating_sub(decoded_len(&image));
            if let Some(cx) = cx.as_deref_mut() {
                cx.drop_image(image, None);
            }
        }
        self.clear_source_projections(key);

        let derived: Vec<DisplayKey> = self
            .display
            .keys()
            .filter(|display| display.dest == key.0)
            .cloned()
            .collect();
        for display in derived {
            match self.display.remove(&display) {
                Some(DisplaySlot::Ready(image)) => {
                    self.display_bytes = self.display_bytes.saturating_sub(image.bytes);
                    self.clear_display_projection(&display);
                    if let Some(cx) = cx.as_deref_mut()
                        && !image.shared_with_source
                    {
                        cx.drop_image(image.image, None);
                    }
                }
                Some(DisplaySlot::InFlight) => {
                    self.display_inflight = self.display_inflight.saturating_sub(1);
                }
                None => {}
            }
            self.display_lru.retain(|candidate| *candidate != display);
            self.display_visible.remove(&display);
        }
    }

    #[cfg(test)]
    pub(super) fn source_bytes(&self) -> usize {
        self.source_bytes
    }

    fn pick_display_victim(&self) -> Option<usize> {
        let evictable = |key: &DisplayKey| {
            !self.display_visible.contains(key)
                && !matches!(self.display.get(key), Some(DisplaySlot::InFlight))
        };
        let warm = |key: &DisplayKey| self.source_warm.contains(key.dest.as_str());
        self.display_lru
            .iter()
            .position(|key| evictable(key) && !warm(key))
            .or_else(|| self.display_lru.iter().position(evictable))
    }

    fn warm_display_count(&self) -> usize {
        self.display
            .keys()
            .filter(|key| {
                !self.display_visible.contains(*key) && self.source_warm.contains(key.dest.as_str())
            })
            .count()
    }

    fn evict_display(&mut self, mut cx: Option<&mut App>, max_entries: usize, max_bytes: usize) {
        while self.display.len() > max_entries || self.display_bytes > max_bytes {
            let Some(idx) = self.pick_display_victim() else {
                break;
            };
            let victim = self.display_lru.remove(idx).expect("lru");
            match self.display.remove(&victim) {
                Some(DisplaySlot::Ready(img)) => {
                    self.display_bytes = self.display_bytes.saturating_sub(img.bytes);
                    self.clear_display_projection(&victim);
                    if let Some(cx) = cx.as_deref_mut()
                        && !img.shared_with_source
                    {
                        cx.drop_image(img.image, None);
                    }
                }
                Some(DisplaySlot::InFlight) => unreachable!(),
                None => {}
            }
        }
    }

    #[cfg(test)]
    pub(super) fn display_entry_count(&self) -> usize {
        self.display.len()
    }

    #[cfg(test)]
    pub(super) fn drop_display_keep_source(&mut self, key: &DisplayKey) {
        if let Some(DisplaySlot::Ready(img)) = self.display.remove(key) {
            self.display_bytes = self.display_bytes.saturating_sub(img.bytes);
            self.clear_display_projection(key);
        }
        self.display_lru.retain(|k| k != key);
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}
