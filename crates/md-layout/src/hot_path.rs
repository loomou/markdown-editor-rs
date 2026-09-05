#[cfg(feature = "hot_path")]
use std::sync::atomic::Ordering;

std::thread_local! {
    static HEIGHT_INDEX_BUILD: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[inline(always)]
pub(crate) fn add_height_index_build() {
    HEIGHT_INDEX_BUILD.with(|n| n.set(n.get() + 1));
}

pub fn height_index_builds() -> u64 {
    HEIGHT_INDEX_BUILD.with(std::cell::Cell::get)
}

#[inline(always)]
pub(crate) fn add_flow_lower() {
    #[cfg(feature = "hot_path")]
    inner::FLOW_LOWER.fetch_add(1, Ordering::Relaxed);
}

#[inline(always)]
pub fn add_store_clear() {
    #[cfg(feature = "hot_path")]
    inner::STORE_CLEAR.fetch_add(1, Ordering::Relaxed);
}

#[inline(always)]
pub fn add_geometry_mismatch() {
    #[cfg(feature = "hot_path")]
    inner::GEOMETRY_MISMATCH.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "hot_path")]
mod inner {
    use super::{HEIGHT_INDEX_BUILD, HotPathSnapshot};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static FLOW_LOWER: AtomicU64 = AtomicU64::new(0);
    pub static STORE_CLEAR: AtomicU64 = AtomicU64::new(0);
    pub static GEOMETRY_MISMATCH: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        FLOW_LOWER.store(0, Ordering::Relaxed);
        HEIGHT_INDEX_BUILD.with(|n| n.set(0));
        STORE_CLEAR.store(0, Ordering::Relaxed);
        GEOMETRY_MISMATCH.store(0, Ordering::Relaxed);
    }

    pub fn snapshot() -> HotPathSnapshot {
        HotPathSnapshot {
            project: 0,
            flow_lower: FLOW_LOWER.load(Ordering::Relaxed),
            height_index_build: super::height_index_builds(),
            adapter_to_block: 0,
            anchor_linear_scan: 0,
            store_clear: STORE_CLEAR.load(Ordering::Relaxed),
            geometry_mismatch: GEOMETRY_MISMATCH.load(Ordering::Relaxed),
        }
    }
}

#[cfg(feature = "hot_path")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HotPathSnapshot {
    pub project: u64,
    pub flow_lower: u64,
    pub height_index_build: u64,
    pub adapter_to_block: u64,
    pub anchor_linear_scan: u64,
    pub store_clear: u64,
    pub geometry_mismatch: u64,
}

#[cfg(feature = "hot_path")]
impl std::fmt::Display for HotPathSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "project={} flow_lower={} height_index_build={} adapter_to_block={} anchor_linear_scan={} store_clear={} geometry_mismatch={}",
            self.project,
            self.flow_lower,
            self.height_index_build,
            self.adapter_to_block,
            self.anchor_linear_scan,
            self.store_clear,
            self.geometry_mismatch,
        )
    }
}

#[cfg(feature = "hot_path")]
pub fn reset() {
    inner::reset();
}

#[cfg(feature = "hot_path")]
pub fn snapshot() -> HotPathSnapshot {
    inner::snapshot()
}
