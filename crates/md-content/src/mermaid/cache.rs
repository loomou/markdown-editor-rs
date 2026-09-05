use super::{
    MAX_BYTES, MAX_ENTRIES, MAX_FITTED_ENTRIES, MAX_IN_FLIGHT, MAX_SVG_ENTRIES, MermaidCache,
    MermaidKey, MermaidSvgKey, Slot, WARM_EXTRA_ENTRIES, WARM_IN_FLIGHT,
};
use crate::pixels::ReadyImage;
use gpui::App;
use md_core::block::BlockId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

impl MermaidCache {
    pub fn new() -> Self {
        MermaidCache {
            map: HashMap::new(),
            lru: VecDeque::new(),
            latest: HashMap::new(),
            hot: HashSet::new(),
            warm: HashSet::new(),
            bytes: 0,
            inflight: 0,
            theme_fp: 0,
            svgs: HashMap::new(),
            svg_order: VecDeque::new(),
            current: Rc::new(HashMap::new()),
            fitted: Rc::new(HashMap::new()),
            fitted_order: VecDeque::new(),
        }
    }

    pub fn svg(&self, key: &MermaidSvgKey) -> Option<Arc<merman::svg::ResvgCompatibleSvg>> {
        self.svgs.get(key).cloned()
    }

    pub fn store_svg(&mut self, key: MermaidSvgKey, svg: Arc<merman::svg::ResvgCompatibleSvg>) {
        if self.svgs.insert(key, svg).is_none() {
            self.svg_order.push_back(key);
        }
        while self.svg_order.len() > MAX_SVG_ENTRIES {
            let Some(victim) = self.svg_order.pop_front() else {
                break;
            };
            self.svgs.remove(&victim);
        }
    }

    pub fn contains(&self, key: &MermaidKey) -> bool {
        self.map.contains_key(key)
    }

    pub fn image(&self, key: &MermaidKey) -> Option<ReadyImage> {
        match self.map.get(key) {
            Some(Slot::Ready(img)) => Some(img.clone()),
            Some(Slot::Failed(_)) => None,
            Some(Slot::InFlight) | None => {
                self.current
                    .get(&(key.block, key.generation))
                    .and_then(|(current, _)| {
                        if let Some(Slot::Ready(img)) = self.map.get(current) {
                            Some(img.clone())
                        } else {
                            None
                        }
                    })
            }
        }
    }

    pub fn failed(&self, key: &MermaidKey) -> bool {
        matches!(self.map.get(key), Some(Slot::Failed(_)))
    }

    pub fn failed_error(&self, key: &MermaidKey) -> Option<&crate::Error> {
        match self.map.get(key) {
            Some(Slot::Failed(err)) => Some(err),
            _ => None,
        }
    }

    pub fn begin(&mut self, key: MermaidKey) -> bool {
        if self.map.contains_key(&key) {
            return false;
        }
        let budget = if self.warm.contains(&key) {
            WARM_IN_FLIGHT
        } else {
            MAX_IN_FLIGHT
        };
        if self.inflight >= budget {
            return false;
        }
        self.latest.insert((key.block, key.generation), key);
        self.map.insert(key, Slot::InFlight);
        self.inflight += 1;
        true
    }

    pub fn set_working_set(
        &mut self,
        hot: impl IntoIterator<Item = MermaidKey>,
        warm: impl IntoIterator<Item = MermaidKey>,
        cx: &mut App,
    ) {
        self.set_working_set_inner(hot, warm, Some(cx));
    }

    pub(super) fn set_working_set_inner(
        &mut self,
        hot: impl IntoIterator<Item = MermaidKey>,
        warm: impl IntoIterator<Item = MermaidKey>,
        cx: Option<&mut App>,
    ) {
        self.hot.clear();
        self.hot.extend(hot);
        self.warm.clear();
        self.warm.extend(warm);

        self.warm.retain(|key| !self.hot.contains(key));

        let ready: Vec<MermaidKey> = self
            .hot
            .iter()
            .copied()
            .filter(|key| matches!(self.map.get(key), Some(Slot::Ready(_))))
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
        key: MermaidKey,
        result: Result<ReadyImage, crate::Error>,
        cx: &mut App,
    ) {
        self.finish_inner(key, result, Some(cx));
    }

    pub(super) fn finish_inner(
        &mut self,
        key: MermaidKey,
        result: Result<ReadyImage, crate::Error>,
        cx: Option<&mut App>,
    ) {
        let Some(slot) = self.map.get(&key) else {
            return;
        };
        if !matches!(slot, Slot::InFlight) {
            return;
        }
        self.inflight = self.inflight.saturating_sub(1);
        match result {
            Ok(img) => {
                if self.latest.get(&(key.block, key.generation)) == Some(&key) {
                    self.set_current((key.block, key.generation), key, img.css_size());
                }
                self.bytes = self.bytes.saturating_add(img.bytes);
                self.map.insert(key, Slot::Ready(img));
                self.touch(key);
            }
            Err(err) => {
                self.map.insert(key, Slot::Failed(err));
                self.touch(key);
            }
        }
        self.evict(cx, MAX_ENTRIES, MAX_BYTES);
    }

    pub fn sync_theme(&mut self, theme_fp: u64, cx: &mut App) {
        if self.theme_fp != theme_fp {
            self.clear(cx);
            self.theme_fp = theme_fp;
        }
    }

    pub fn clear(&mut self, cx: &mut App) {
        for slot in self.map.values() {
            if let Slot::Ready(img) = slot {
                cx.drop_image(img.image.clone(), None);
            }
        }
        self.map.clear();
        self.lru.clear();
        self.latest.clear();
        self.hot.clear();
        self.warm.clear();
        self.bytes = 0;
        self.inflight = 0;
        self.svgs.clear();
        self.svg_order.clear();
        self.current = Rc::new(HashMap::new());
        self.fitted = Rc::new(HashMap::new());
        self.fitted_order.clear();
    }

    pub fn fitted_snapshot(&self) -> Rc<HashMap<(BlockId, u32), (f32, f32)>> {
        Rc::clone(&self.fitted)
    }

    fn touch(&mut self, key: MermaidKey) {
        self.lru.retain(|k| *k != key);
        self.lru.push_back(key);
    }

    fn pick_victim(&self) -> Option<MermaidKey> {
        let evictable = |k: &MermaidKey| {
            !self.hot.contains(k) && !matches!(self.map.get(k), Some(Slot::InFlight))
        };
        self.lru
            .iter()
            .find(|k| evictable(k) && !self.warm.contains(k))
            .or_else(|| self.lru.iter().find(|k| evictable(k)))
            .copied()
    }

    fn evict(&mut self, mut cx: Option<&mut App>, max_entries: usize, max_bytes: usize) {
        while self.map.len() > max_entries || self.bytes > max_bytes {
            let Some(victim) = self.pick_victim() else {
                break;
            };
            self.lru.retain(|k| *k != victim);
            match self.map.remove(&victim) {
                Some(Slot::Ready(img)) => {
                    self.bytes = self.bytes.saturating_sub(img.bytes);
                    if let Some(cx) = cx.as_deref_mut() {
                        cx.drop_image(img.image, None);
                    }
                }
                Some(Slot::Failed(_)) => {}
                Some(Slot::InFlight) => unreachable!(),
                None => {}
            }
            let identity = (victim.block, victim.generation);
            if self
                .current
                .get(&identity)
                .is_some_and(|(current, _)| current == &victim)
            {
                self.remove_current(identity);
            }
            if self.latest.get(&identity) == Some(&victim) {
                self.latest.remove(&identity);
            }
        }
    }

    fn set_current(&mut self, identity: (BlockId, u32), key: MermaidKey, size: (f32, f32)) {
        Rc::make_mut(&mut self.current).insert(identity, (key, size));
        Rc::make_mut(&mut self.fitted).insert(identity, size);
        self.fitted_order.retain(|id| *id != identity);
        self.fitted_order.push_back(identity);
        self.trim_fitted();
    }

    fn remove_current(&mut self, identity: (BlockId, u32)) {
        Rc::make_mut(&mut self.current).remove(&identity);
    }

    fn trim_fitted(&mut self) {
        while self.fitted_order.len() > MAX_FITTED_ENTRIES {
            let Some(victim) = self.fitted_order.pop_front() else {
                break;
            };
            Rc::make_mut(&mut self.fitted).remove(&victim);
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

impl Default for MermaidCache {
    fn default() -> Self {
        Self::new()
    }
}
