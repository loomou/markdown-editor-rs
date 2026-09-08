use gpui::{ImageFormat, RenderImage};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_IN_FLIGHT: usize = 4;

const WARM_SOURCE_IN_FLIGHT: usize = 2;

const MAX_DISPLAY_ENTRIES: usize = 48;
const MAX_DISPLAY_BYTES: usize = 96 * 1024 * 1024;
const WARM_DISPLAY_EXTRA: usize = 8;
const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
const MAX_FETCH: usize = 16 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
const HTTP_TIMEOUT_SECS: u64 = 15;

const MAX_FAILED_ENTRIES: usize = 256;

const SOURCE_RETRY_BASE: Duration = Duration::from_secs(4);
const SOURCE_RETRY_MAX: Duration = Duration::from_secs(64);

fn source_retry_delay(attempt: u32) -> Duration {
    SOURCE_RETRY_BASE
        .saturating_mul(1 << attempt.min(6))
        .min(SOURCE_RETRY_MAX)
}

type SourceLoad =
    Pin<Box<dyn Future<Output = Result<(Arc<RenderImage>, u32, u32), SourceError>> + Send>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    Fatal(crate::Error),
    Retryable {
        err: crate::Error,
        retry_after: Option<Duration>,
    },
}

impl SourceError {
    pub fn retryable(err: crate::Error) -> Self {
        SourceError::Retryable {
            err,
            retry_after: None,
        }
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Fatal(err) | SourceError::Retryable { err, .. } => err.fmt(f),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<crate::Error> for SourceError {
    fn from(err: crate::Error) -> Self {
        SourceError::Fatal(err)
    }
}

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
    Network(String),
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
    InFlight {
        retry: u32,
        seq: u64,
    },
    Ready {
        image: Arc<RenderImage>,
    },
    Failed(crate::Error),
    Retryable {
        err: crate::Error,
        attempt: u32,
        retry_at: Instant,
    },
    OverBudget {
        err: crate::Error,
        wanted: usize,
    },
}

enum DisplaySlot {
    InFlight { seq: u64 },
    Ready(ReadyImage),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceToken {
    pub dest: String,
    seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayToken {
    pub key: DisplayKey,
    seq: u64,
}

pub struct ImageCache {
    source: HashMap<SourceKey, SourceSlot>,
    source_lru: VecDeque<SourceKey>,
    source_visible: HashSet<SourceKey>,
    source_warm: HashSet<SourceKey>,
    source_bytes: usize,
    source_inflight: usize,
    job_seq: u64,
    failed_lru: VecDeque<SourceKey>,
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
