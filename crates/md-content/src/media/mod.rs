use crate::images::{DisplayKey, ImageCache};
use crate::math::{MathCache, MathKey};
use crate::mermaid::{MermaidCache, MermaidKey};
use crate::pixels::AllocationLedger;
use gpui::{App, RenderImage};
use std::sync::Arc;

pub use crate::pixels::drop_image_deferred;

pub const DEFAULT_TOTAL_BYTES: usize = 352 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaLayer {
    Mermaid,
    Math,
    Display,
    Source,
}

const LAYERS: [MediaLayer; 4] = [
    MediaLayer::Mermaid,
    MediaLayer::Math,
    MediaLayer::Display,
    MediaLayer::Source,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaStats {
    pub mermaid_bytes: usize,
    pub math_bytes: usize,
    pub display_bytes: usize,
    pub source_bytes: usize,
}

impl MediaStats {
    pub fn total(self) -> usize {
        self.mermaid_bytes
            .saturating_add(self.math_bytes)
            .saturating_add(self.display_bytes)
            .saturating_add(self.source_bytes)
    }

    pub fn layer(self, layer: MediaLayer) -> usize {
        match layer {
            MediaLayer::Mermaid => self.mermaid_bytes,
            MediaLayer::Math => self.math_bytes,
            MediaLayer::Display => self.display_bytes,
            MediaLayer::Source => self.source_bytes,
        }
    }
}

pub struct MediaWorkingSet {
    pub mermaid_hot: Vec<MermaidKey>,
    pub mermaid_warm: Vec<MermaidKey>,
    pub math_hot: Vec<MathKey>,
    pub image_hot: Vec<DisplayKey>,
    pub image_hot_sources: Vec<String>,
    pub image_warm_sources: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaReport {
    pub budget: usize,
    pub cache_bytes: usize,
    pub resident_bytes: usize,
    pub over_budget: usize,
}

impl MediaReport {
    pub fn untracked_bytes(self) -> usize {
        self.resident_bytes.saturating_sub(self.cache_bytes)
    }
}

pub struct MediaCache {
    mermaid: MermaidCache,
    math: MathCache,
    images: ImageCache,
    total_bytes: usize,
    ledger: AllocationLedger,
}

impl MediaCache {
    pub fn new() -> Self {
        Self::with_total_bytes(DEFAULT_TOTAL_BYTES)
    }

    pub fn with_total_bytes(total_bytes: usize) -> Self {
        MediaCache {
            mermaid: MermaidCache::new(),
            math: MathCache::new(),
            images: ImageCache::new(),
            total_bytes,
            ledger: AllocationLedger::new(),
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn set_total_bytes(&mut self, total_bytes: usize) {
        self.total_bytes = total_bytes;
    }

    pub fn mermaid(&self) -> &MermaidCache {
        &self.mermaid
    }

    pub fn mermaid_mut(&mut self) -> &mut MermaidCache {
        &mut self.mermaid
    }

    pub fn math(&self) -> &MathCache {
        &self.math
    }

    pub fn math_mut(&mut self) -> &mut MathCache {
        &mut self.math
    }

    pub fn images(&self) -> &ImageCache {
        &self.images
    }

    pub fn images_mut(&mut self) -> &mut ImageCache {
        &mut self.images
    }

    pub fn stats(&self) -> MediaStats {
        MediaStats {
            mermaid_bytes: self.mermaid.bytes(),
            math_bytes: self.math.bytes(),
            display_bytes: self.images.display_bytes(),
            source_bytes: self.images.source_bytes(),
        }
    }

    pub fn track(&mut self, image: &Arc<RenderImage>, bytes: usize) {
        self.ledger.track(image, bytes);
    }

    pub fn settle(&mut self) -> usize {
        let MediaCache {
            mermaid,
            math,
            images,
            ledger,
            ..
        } = self;
        mermaid.for_each_allocation(&mut |image, bytes| ledger.track(image, bytes));
        math.for_each_allocation(&mut |image, bytes| ledger.track(image, bytes));
        images.for_each_allocation(&mut |image, bytes| ledger.track(image, bytes));
        ledger.settle()
    }

    pub fn resident_bytes(&mut self) -> usize {
        self.settle()
    }

    pub fn report(&mut self) -> MediaReport {
        let cache_bytes = self.stats().total();
        let resident_bytes = self.settle();
        MediaReport {
            budget: self.total_bytes,
            cache_bytes,
            resident_bytes,
            over_budget: resident_bytes.saturating_sub(self.total_bytes),
        }
    }

    pub fn submit_working_set(&mut self, set: MediaWorkingSet, cx: &mut App) {
        self.mermaid
            .set_working_set(set.mermaid_hot, set.mermaid_warm, cx);
        self.math
            .set_working_set(set.math_hot, std::iter::empty(), cx);
        self.images.set_working_set(
            set.image_hot,
            set.image_hot_sources,
            set.image_warm_sources,
            cx,
        );
        self.enforce(cx);
    }

    pub fn clear(&mut self, cx: &mut App) {
        self.mermaid.clear(cx);
        self.math.clear(cx);
        self.images.clear(cx);
        self.settle();
    }

    pub fn enforce(&mut self, cx: &mut App) -> usize {
        self.enforce_inner(Some(cx))
    }

    pub(super) fn enforce_inner(&mut self, mut cx: Option<&mut App>) -> usize {
        self.settle();
        let before = self.stats().total();
        if before <= self.total_bytes {
            return 0;
        }
        let mut layers = LAYERS;
        layers.sort_by_key(|layer| std::cmp::Reverse(self.stats().layer(*layer)));
        for layer in layers {
            let stats = self.stats();
            let excess = stats.total().saturating_sub(self.total_bytes);
            if excess == 0 {
                break;
            }
            let target = stats.layer(layer).saturating_sub(excess);
            let cx = cx.as_deref_mut();
            match layer {
                MediaLayer::Mermaid => self.mermaid.shrink_to(target, cx),
                MediaLayer::Math => self.math.shrink_to(target, cx),
                MediaLayer::Display => self.images.shrink_display_to(target, cx),
                MediaLayer::Source => self.images.shrink_source_to(target, cx),
            }
        }
        self.settle();
        before.saturating_sub(self.stats().total())
    }
}

impl Default for MediaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
