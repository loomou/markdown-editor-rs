use gpui::ImageFormat;
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
/// A raster source larger than this is decoded downscaled. Every raster the
/// editor builds (display slot or zoom overlay) is capped here, so the worst
/// case a huge screenshot can cost is one 4M-pixel buffer instead of its
/// declared canvas.
const MAX_RASTER_PIXELS: u64 = 4 * 1024 * 1024;
const HTTP_TIMEOUT_SECS: u64 = 15;

/// Working bytes a source load asks for before it knows the file size. The read
/// buffer is capped at `MAX_FETCH`, so that is the most it can hold while the
/// decode that follows is estimated separately per raster.
pub fn estimate_source_job() -> usize {
    MAX_FETCH
}

const MAX_FAILED_ENTRIES: usize = 256;

const DISPLAY_SCREENS: usize = 8;
const SOURCE_SCREENS: usize = 8;
const MIN_DISPLAY_BYTES: usize = 48 * 1024 * 1024;
const MIN_SOURCE_BYTES: usize = 96 * 1024 * 1024;
const VIEWPORT_DISPLAY_ENTRIES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub display_bytes: usize,
    pub display_entries: usize,
    pub source_bytes: usize,
}

impl Budget {
    pub fn for_viewport(width: f64, height: f64, dpr: f64) -> Self {
        let dpr = dpr.max(0.25);
        let screen = (width.max(1.0) * height.max(1.0) * dpr * dpr * 4.0) as usize;
        Budget {
            display_bytes: screen
                .saturating_mul(DISPLAY_SCREENS)
                .max(MIN_DISPLAY_BYTES),
            display_entries: VIEWPORT_DISPLAY_ENTRIES,
            source_bytes: screen.saturating_mul(SOURCE_SCREENS).max(MIN_SOURCE_BYTES),
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            display_bytes: MAX_DISPLAY_BYTES,
            display_entries: MAX_DISPLAY_ENTRIES,
            source_bytes: MAX_SOURCE_BYTES,
        }
    }
}

const SOURCE_RETRY_BASE: Duration = Duration::from_secs(4);
const SOURCE_RETRY_MAX: Duration = Duration::from_secs(64);

const WARM_MIN_GAIN_PERCENT: usize = crate::media::COOL_MIN_GAIN_PERCENT;

fn source_retry_delay(attempt: u32) -> Duration {
    SOURCE_RETRY_BASE
        .saturating_mul(1 << attempt.min(6))
        .min(SOURCE_RETRY_MAX)
}

type SourceLoad = Pin<Box<dyn Future<Output = Result<SourceMedia, SourceError>> + Send>>;

/// What a resolved image source keeps in the cache.
///
/// Neither variant holds decoded pixels. Raster formats keep the encoded file
/// plus its intrinsic size and decode per display slot; a 3 MiB PNG of 33 MiB
/// of RGBA therefore costs 3 MiB while it is only being scrolled past. SVG
/// keeps the parse tree instead of a prerasterised bitmap: a bitmap rasterised
/// at intrinsic size scales with the declared canvas (a 1299x1299 svg costs
/// 27 MiB at dpr 2) while the tree is a few KB and renders per slot on demand.
#[derive(Clone, Debug)]
pub enum SourceMedia {
    Bitmap {
        bytes: Arc<Vec<u8>>,
        format: ImageFormat,
        width: u32,
        height: u32,
        /// The size the file decodes to, before the display cap. A job pays
        /// for these pixels, not for the capped ones it keeps.
        decoded: (u32, u32),
        lease: Option<Arc<crate::media::ByteLease>>,
    },
    Vector {
        tree: Arc<usvg::Tree>,
        width: u32,
        height: u32,
        bytes: usize,
        lease: Option<Arc<crate::media::ByteLease>>,
    },
}

impl SourceMedia {
    pub fn dims(&self) -> (u32, u32) {
        match self {
            SourceMedia::Bitmap { width, height, .. } => (*width, *height),
            SourceMedia::Vector { width, height, .. } => (*width, *height),
        }
    }

    /// Pixels a raster job of this source has to allocate for its decode.
    pub fn decoded_dims(&self) -> (u32, u32) {
        match self {
            SourceMedia::Bitmap { decoded, .. } => *decoded,
            SourceMedia::Vector { width, height, .. } => (*width, *height),
        }
    }

    pub fn resident_bytes(&self) -> usize {
        match self {
            SourceMedia::Bitmap { bytes, .. } => bytes.len(),
            SourceMedia::Vector { bytes, .. } => *bytes,
        }
    }

    /// Charge the encoded bytes or the parse tree to the resident account for
    /// as long as any clone of this source is alive.
    pub fn with_lease(mut self, lease: Arc<crate::media::ByteLease>) -> Self {
        match &mut self {
            SourceMedia::Bitmap { lease: slot, .. } | SourceMedia::Vector { lease: slot, .. } => {
                *slot = Some(lease)
            }
        }
        self
    }

    pub fn lease(&self) -> Option<&Arc<crate::media::ByteLease>> {
        match self {
            SourceMedia::Bitmap { lease, .. } | SourceMedia::Vector { lease, .. } => lease.as_ref(),
        }
    }

    pub fn allocation_id(&self) -> Option<u64> {
        self.lease().map(|lease| lease.id())
    }
}

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
mod warm;

#[cfg(test)]
mod tests;

pub use crate::pixels::{ReadyImage, dpr_from_q, dpr_q, snap_css};
pub use fit::{contain_fit, width_fit};
pub use keys::{cache_key, display_key, quantized_slot};
pub use net::http_client;
pub use raster::{estimate_display_job, estimate_zoom_job, paint_ready, raster_at, raster_display};
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
        media: SourceMedia,
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
    InFlight {
        seq: u64,
    },
    Ready(ReadyImage),
    /// Pixels replaced by their compressed form because the entry left the
    /// screen. The entry still exists: a re-visit inflates it instead of
    /// decoding the source and rasterising it again.
    Warm(crate::media::WarmImage),
    /// An inflation job is running for this key, so nothing may start a second
    /// one or a raster behind its back.
    Thawing {
        seq: u64,
        warm: crate::media::WarmImage,
    },
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

/// Names one compression job so a result that arrives after the entry has been
/// painted, replaced or dropped cannot install itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoolToken {
    pub key: DisplayKey,
    seq: u64,
}

pub struct ImageCache {
    budget: Budget,
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
    warm_bytes: usize,
    display_inflight: usize,
    cooling: HashMap<DisplayKey, u64>,
    left_screen: HashMap<DisplayKey, Instant>,
    cool_refused: HashSet<DisplayKey>,
    cool_discarded: usize,
    /// Set the first time an entry is dropped for lack of room. Compressing an
    /// off-screen entry only buys anything when the content does not fit the
    /// slice; a cache that has never had to evict is holding everything it was
    /// asked for, and a revisit there is already free.
    spilled: bool,
    sizes_gen: u64,
    sizes: Rc<HashMap<SourceKey, (u32, u32)>>,
    ready: Rc<HashMap<DisplayKey, ReadyImage>>,
    failed_src: Rc<HashSet<SourceKey>>,
}
