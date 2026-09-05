use super::{
    MAX_BYTES, MAX_ENTRIES, MAX_IN_FLIGHT, MAX_METRIC_ENTRIES, MathCache, MathKey, MathMetrics,
    RasterOut, Slot, WARM_EXTRA_ENTRIES, metric,
};
use crate::pixels::ReadyImage;
use gpui::App;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

impl MathCache {
    pub fn new() -> Self {
        MathCache {
            map: HashMap::new(),
            lru: VecDeque::new(),
            hot: HashSet::new(),
            warm: HashSet::new(),
            bytes: 0,
            inflight: 0,
            metrics: Rc::new(HashMap::new()),
            metrics_order: VecDeque::new(),
            metrics_gen: 0,
            ready: Rc::new(HashMap::new()),
            failed: Rc::new(HashSet::new()),
        }
    }

    pub fn contains(&self, key: &MathKey) -> bool {
        self.map.contains_key(key)
    }

    pub fn ready_snapshot(&self) -> Rc<HashMap<MathKey, ReadyImage>> {
        Rc::clone(&self.ready)
    }

    pub fn failed_snapshot(&self) -> Rc<HashSet<MathKey>> {
        Rc::clone(&self.failed)
    }

    pub fn failed_error(&self, key: &MathKey) -> Option<&crate::Error> {
        match self.map.get(key) {
            Some(Slot::Failed(err)) => Some(err),
            _ => None,
        }
    }

    pub fn begin(&mut self, key: MathKey) -> bool {
        if self.map.contains_key(&key) {
            return false;
        }
        if self.inflight >= MAX_IN_FLIGHT {
            return false;
        }
        self.map.insert(key, Slot::InFlight);
        self.inflight += 1;
        true
    }

    pub fn set_working_set(
        &mut self,
        hot: impl IntoIterator<Item = MathKey>,
        warm: impl IntoIterator<Item = MathKey>,
        cx: &mut App,
    ) {
        self.set_working_set_inner(hot, warm, Some(cx));
    }

    pub(super) fn set_working_set_inner(
        &mut self,
        hot: impl IntoIterator<Item = MathKey>,
        warm: impl IntoIterator<Item = MathKey>,
        cx: Option<&mut App>,
    ) {
        self.hot.clear();
        self.hot.extend(hot);
        self.warm.clear();
        self.warm.extend(warm);

        self.warm.retain(|key| !self.hot.contains(key));

        let ready: Vec<MathKey> = self
            .hot
            .iter()
            .filter(|key| matches!(self.map.get(*key), Some(Slot::Ready(_))))
            .cloned()
            .collect();
        for key in ready {
            self.touch(key);
        }
        let working = self.hot.len().saturating_add(self.warm.len());
        let warm_entries = working.saturating_add(WARM_EXTRA_ENTRIES).min(MAX_ENTRIES);
        self.evict(cx, warm_entries, MAX_BYTES);
    }

    pub fn finish(
        &mut self,
        key: MathKey,
        result: Result<RasterOut, crate::Error>,
        cx: &mut App,
    ) -> bool {
        self.finish_inner(key, result, Some(cx))
    }

    pub(super) fn finish_inner(
        &mut self,
        key: MathKey,
        result: Result<RasterOut, crate::Error>,
        cx: Option<&mut App>,
    ) -> bool {
        let Some(slot) = self.map.get(&key) else {
            return false;
        };
        if !matches!(slot, Slot::InFlight) {
            return false;
        }
        self.inflight = self.inflight.saturating_sub(1);
        let changed = match result {
            Ok(out) => {
                let metrics_changed = match metric(&self.metrics, &key.latex, key.display) {
                    Some(Some(prev)) => {
                        prev.width.to_bits() != out.em.width.to_bits()
                            || prev.height.to_bits() != out.em.height.to_bits()
                            || prev.depth.to_bits() != out.em.depth.to_bits()
                    }
                    _ => true,
                };
                Rc::make_mut(&mut self.metrics)
                    .entry(key.latex.clone())
                    .or_default()
                    .set(key.display, Some(out.em));
                self.touch_metric(&key);
                if metrics_changed {
                    self.metrics_gen = self.metrics_gen.wrapping_add(1);
                }
                match out.image {
                    Some(img) => {
                        self.bytes = self.bytes.saturating_add(img.bytes);
                        self.map.insert(key.clone(), Slot::Ready(img.clone()));
                        Rc::make_mut(&mut self.ready).insert(key.clone(), img);
                        self.touch(key);
                    }
                    None => {
                        self.map.insert(
                            key.clone(),
                            Slot::Failed(crate::Error::Math(
                                md_i18n::Key::FormulaRenderFailed.into(),
                            )),
                        );
                        Rc::make_mut(&mut self.failed).insert(key.clone());
                        self.touch(key);
                    }
                }
                true
            }
            Err(err) => {
                let changed = self.record_failure(&key);
                self.map.insert(key.clone(), Slot::Failed(err));
                Rc::make_mut(&mut self.failed).insert(key.clone());
                self.touch(key);
                changed
            }
        };
        self.evict(cx, MAX_ENTRIES, MAX_BYTES);
        changed
    }

    pub fn clear(&mut self, cx: &mut App) {
        for slot in self.map.values() {
            if let Slot::Ready(img) = slot {
                cx.drop_image(img.image.clone(), None);
            }
        }
        self.map.clear();
        self.lru.clear();
        self.hot.clear();
        self.warm.clear();
        self.bytes = 0;
        self.inflight = 0;
        self.metrics = Rc::new(HashMap::new());
        self.metrics_order.clear();
        self.metrics_gen = 0;
        self.ready = Rc::new(HashMap::new());
        self.failed = Rc::new(HashSet::new());
    }

    pub fn metrics_snapshot(&self) -> Rc<MathMetrics> {
        Rc::clone(&self.metrics)
    }

    pub fn metrics_gen(&self) -> u64 {
        self.metrics_gen
    }

    pub(super) fn record_failure(&mut self, key: &MathKey) -> bool {
        if matches!(metric(&self.metrics, &key.latex, key.display), Some(None)) {
            return false;
        }
        let metrics = Rc::make_mut(&mut self.metrics);
        metrics
            .entry(key.latex.clone())
            .or_default()
            .set(key.display, None);
        self.touch_metric(key);
        self.metrics_gen = self.metrics_gen.wrapping_add(1);
        true
    }

    fn touch(&mut self, key: MathKey) {
        self.lru.retain(|k| *k != key);
        self.lru.push_back(key);
    }

    fn victim_index(&self) -> Option<usize> {
        let evictable =
            |k: &MathKey| !self.hot.contains(k) && !matches!(self.map.get(k), Some(Slot::InFlight));
        self.lru
            .iter()
            .position(|k| evictable(k) && !self.warm.contains(k))
            .or_else(|| self.lru.iter().position(evictable))
    }

    fn evict(&mut self, mut cx: Option<&mut App>, max_entries: usize, max_bytes: usize) {
        while self.map.len() > max_entries || self.bytes > max_bytes {
            let Some(idx) = self.victim_index() else {
                break;
            };
            let victim = self.lru.remove(idx).expect("lru");
            match self.map.remove(&victim) {
                Some(Slot::Ready(img)) => {
                    self.bytes = self.bytes.saturating_sub(img.bytes);
                    if let Some(cx) = cx.as_deref_mut() {
                        cx.drop_image(img.image, None);
                    }
                    Rc::make_mut(&mut self.ready).remove(&victim);
                }
                Some(Slot::Failed(_)) => {
                    Rc::make_mut(&mut self.failed).remove(&victim);
                }
                Some(Slot::InFlight) => unreachable!(),
                None => {}
            }
        }
    }

    fn touch_metric(&mut self, key: &MathKey) {
        let id = (key.latex.clone(), key.display);
        self.metrics_order.retain(|entry| *entry != id);
        self.metrics_order.push_back(id);
        self.trim_metrics();
    }

    fn trim_metrics(&mut self) {
        while self.metrics_order.len() > MAX_METRIC_ENTRIES {
            let Some((latex, display)) = self.metrics_order.pop_front() else {
                break;
            };
            let metrics = Rc::make_mut(&mut self.metrics);
            let Some(slots) = metrics.get_mut(&latex) else {
                continue;
            };
            slots.remove(display);
            if slots.is_empty() {
                metrics.remove(&latex);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn entry_count(&self) -> usize {
        self.map.len()
    }

    #[cfg(test)]
    pub(super) fn byte_count(&self) -> usize {
        self.bytes
    }
}

impl Default for MathCache {
    fn default() -> Self {
        Self::new()
    }
}
