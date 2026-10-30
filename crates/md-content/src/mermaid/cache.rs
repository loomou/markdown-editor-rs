use super::{
    Budget, MAX_FITTED_ENTRIES, MAX_IN_FLIGHT, MAX_SVG_ENTRIES, MermaidCache, MermaidIdentity,
    MermaidKey, MermaidRasterKey, MermaidSvgKey, Slot, WARM_EXTRA_ENTRIES, WARM_IN_FLIGHT,
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
            budget: Budget::default(),
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

    pub fn set_budget(&mut self, budget: Budget, cx: &mut App) {
        self.set_budget_inner(budget, Some(cx));
    }

    pub(super) fn set_budget_inner(&mut self, budget: Budget, cx: Option<&mut App>) {
        if self.budget == budget {
            return;
        }
        self.budget = budget;
        self.evict(cx, budget.entries, budget.bytes);
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
        self.map.contains_key(&key.raster)
    }

    pub fn image(&self, key: &MermaidKey) -> Option<ReadyImage> {
        match self.map.get(&key.raster) {
            Some(Slot::Ready(img)) => Some(img.clone()),
            Some(Slot::Failed(_)) => None,
            Some(Slot::InFlight) | None => {
                self.current
                    .get(&(key.block, key.generation))
                    .and_then(|(current, _)| match self.map.get(current) {
                        Some(Slot::Ready(img)) => Some(img.clone()),
                        _ => None,
                    })
            }
        }
    }

    pub fn failed(&self, key: &MermaidKey) -> bool {
        matches!(self.map.get(&key.raster), Some(Slot::Failed(_)))
    }

    pub fn failed_error(&self, key: &MermaidKey) -> Option<&crate::Error> {
        match self.map.get(&key.raster) {
            Some(Slot::Failed(err)) => Some(err),
            _ => None,
        }
    }

    pub fn begin(&mut self, key: MermaidKey) -> bool {
        if self.map.contains_key(&key.raster) {
            return false;
        }
        let budget = if self.warm.contains(&key.raster) {
            WARM_IN_FLIGHT
        } else {
            MAX_IN_FLIGHT
        };
        if self.inflight >= budget {
            return false;
        }
        self.latest.insert((key.block, key.generation), key.raster);
        self.map.insert(key.raster, Slot::InFlight);
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
        self.warm.clear();
        let hot: Vec<MermaidKey> = hot.into_iter().collect();
        let warm: Vec<MermaidKey> = warm.into_iter().collect();
        self.hot.extend(hot.iter().map(|key| key.raster));
        self.warm.extend(warm.iter().map(|key| key.raster));
        self.warm.retain(|key| !self.hot.contains(key));
        let ready: Vec<(MermaidIdentity, MermaidRasterKey, (f32, f32))> = hot
            .iter()
            .filter_map(|key| match self.map.get(&key.raster) {
                Some(Slot::Ready(img)) => {
                    Some(((key.block, key.generation), key.raster, img.css_size()))
                }
                _ => None,
            })
            .collect();
        for (identity, raster, size) in ready {
            self.set_current(identity, raster, size);
            self.touch(raster);
        }
        let working = self.hot.len().saturating_add(self.warm.len());
        let warm_entries = working
            .saturating_add(WARM_EXTRA_ENTRIES)
            .min(self.budget.entries);
        let bytes = self.budget.bytes;
        self.evict(cx, warm_entries, bytes);
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
        let Some(slot) = self.map.get(&key.raster) else {
            return;
        };
        if !matches!(slot, Slot::InFlight) {
            return;
        }
        self.inflight = self.inflight.saturating_sub(1);
        let identity = (key.block, key.generation);
        match result {
            Ok(img) => {
                if self.latest.get(&identity) == Some(&key.raster) {
                    self.set_current(identity, key.raster, img.css_size());
                }
                self.bytes = self.bytes.saturating_add(img.bytes);
                self.map.insert(key.raster, Slot::Ready(img));
                self.touch(key.raster);
            }
            Err(err) => {
                self.map.insert(key.raster, Slot::Failed(err));
                self.touch(key.raster);
            }
        }
        let entries = self.budget.entries;
        let bytes = self.budget.bytes;
        self.evict(cx, entries, bytes);
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
                crate::media::drop_image_deferred(cx, img.image.clone());
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

    fn touch(&mut self, key: MermaidRasterKey) {
        self.lru.retain(|k| *k != key);
        self.lru.push_back(key);
    }

    fn pick_victim(&self) -> Option<MermaidRasterKey> {
        let evictable = |k: &MermaidRasterKey| {
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
                        crate::media::drop_image_deferred(cx, img.image);
                    }
                }
                Some(Slot::Failed(_)) => {}
                Some(Slot::InFlight) => unreachable!(),
                None => {}
            }
            Rc::make_mut(&mut self.current).retain(|_, (current, _)| current != &victim);
            self.latest.retain(|_, current| current != &victim);
        }
    }

    fn set_current(&mut self, identity: MermaidIdentity, key: MermaidRasterKey, size: (f32, f32)) {
        if self
            .current
            .get(&identity)
            .is_some_and(|(current, known)| *current == key && *known == size)
        {
            return;
        }
        Rc::make_mut(&mut self.current).insert(identity, (key, size));
        Rc::make_mut(&mut self.fitted).insert(identity, size);
        self.fitted_order.retain(|id| *id != identity);
        self.fitted_order.push_back(identity);
        self.trim_fitted();
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

    pub fn account(&self) -> crate::media::Account {
        crate::media::Account {
            mermaid_bytes: self.bytes,
            mermaid_entries: self.map.len(),
            ..crate::media::Account::default()
        }
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
