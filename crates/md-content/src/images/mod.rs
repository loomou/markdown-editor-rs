use gpui::{ImageFormat, RenderImage};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

const MAX_IN_FLIGHT: usize = 4;

const WARM_SOURCE_IN_FLIGHT: usize = 2;

const MAX_DISPLAY_ENTRIES: usize = 48;
const MAX_DISPLAY_BYTES: usize = 96 * 1024 * 1024;
const WARM_DISPLAY_EXTRA: usize = 8;
const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_FETCH: usize = 16 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
const HTTP_TIMEOUT_SECS: u64 = 15;

type SourceLoad =
    Pin<Box<dyn Future<Output = Result<(Arc<RenderImage>, u32, u32), crate::Error>> + Send>>;

mod cache;
mod fit;
mod keys;
mod net;
mod raster;
mod source;

#[cfg(test)]
mod tests;

pub use crate::pixels::{ReadyImage, dpr_from_q, dpr_q, snap_css};
pub use fit::contain_fit;
pub(crate) use fit::width_fit;
pub(crate) use keys::quantized_slot;
pub use keys::{cache_key, display_key};
pub use net::http_client;
pub use raster::{paint_ready, raster_display};
pub use source::{is_image_path, load_source, markdown_dest, resolve};

#[derive(Clone, Debug)]
pub enum Resolved {
    Remote(String),
    Local(PathBuf),
    Data {
        payload: String,
        format: ImageFormat,
        encoding: DataEncoding,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataEncoding {
    Base64,
    Percent,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DisplayKey {
    pub dest: String,
    pub width_q: u32,
    pub height_q: u32,
    pub dpr_q: u16,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SourceKey(String);

impl SourceKey {
    pub fn new(dest: impl Into<String>) -> Self {
        SourceKey(dest.into())
    }
}

impl std::borrow::Borrow<str> for SourceKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

enum SourceSlot {
    InFlight,
    Ready { image: Arc<RenderImage> },
    Failed(crate::Error),

    OverBudget { err: crate::Error, wanted: usize },
}

enum DisplaySlot {
    InFlight,
    Ready(ReadyImage),
}

pub struct ImageCache {
    source: HashMap<SourceKey, SourceSlot>,
    source_lru: VecDeque<SourceKey>,
    source_visible: HashSet<SourceKey>,

    source_warm: HashSet<SourceKey>,
    source_bytes: usize,
    source_inflight: usize,
    display: HashMap<DisplayKey, DisplaySlot>,
    display_lru: VecDeque<DisplayKey>,
    display_visible: HashSet<DisplayKey>,
    display_bytes: usize,
    display_inflight: usize,
    sizes_gen: u64,

    sizes: Rc<HashMap<SourceKey, (u32, u32)>>,
    ready: Rc<HashMap<DisplayKey, ReadyImage>>,
    failed_src: Rc<HashSet<SourceKey>>,
}
