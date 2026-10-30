mod anchor;
mod build;
mod changes;
mod engine;
mod estimate;
mod scroll;
mod store;

#[cfg(test)]
mod tests;

pub use anchor::ScrollAnchor;
#[cfg(feature = "bench-support")]
pub use engine::set_keep_screens;
pub use engine::{
    DEFAULT_KEEP_SCREENS, DeferredSettlement, IncrementalEngine, MAX_ITERATIONS, PublishedFrame,
    keep_screens,
};
pub use estimate::Estimator;
pub use store::{EvictionPolicy, MaterializedStore};
