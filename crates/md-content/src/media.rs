use gpui::{App, RenderImage};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) fn drop_image_deferred(cx: &mut App, image: Arc<RenderImage>) {
    cx.defer(move |cx| cx.drop_image(image, None));
}

/// Working bytes a bitmap raster will hold at its peak: the decoded source,
/// the intermediate the resize allocates at the destination width, and the
/// output. The intermediate is the term that made a 336 KiB file peak at
/// 181 MiB, so it is estimated rather than ignored.
pub fn estimate_bitmap_peak(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> usize {
    let src = (src_w as usize)
        .saturating_mul(src_h as usize)
        .saturating_mul(4);
    let intermediate = (dst_w as usize)
        .saturating_mul(src_h as usize)
        .saturating_mul(16);
    let out = (dst_w as usize)
        .saturating_mul(dst_h as usize)
        .saturating_mul(4);
    src.saturating_add(intermediate).saturating_add(out)
}

/// Working bytes a vector raster will hold at its peak: the pixmap resvg
/// renders into plus the BGRA copy handed to gpui.
pub fn estimate_raster_peak(px_w: u32, px_h: u32) -> usize {
    (px_w as usize)
        .saturating_mul(px_h as usize)
        .saturating_mul(4)
        .saturating_mul(2)
}

/// Ceiling on the working bytes of all in-flight media jobs. This is not a
/// cache budget: a job's scratch memory is live only while it runs, but
/// nothing bounded it before, so twelve concurrent decodes could hold twelve
/// peaks at once.
pub const DEFAULT_JOB_BYTES: usize = 128 * 1024 * 1024;

pub struct MediaCoordinator {
    limit: usize,
    reserved: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl gpui::Global for MediaCoordinator {}

impl MediaCoordinator {
    pub fn new(limit: usize) -> Self {
        MediaCoordinator {
            limit,
            reserved: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn install(cx: &mut App, limit: usize) {
        if cx.try_global::<MediaCoordinator>().is_none() {
            cx.set_global(MediaCoordinator::new(limit));
        }
    }

    /// Replace the coordinator instead of installing it once. A probe that
    /// compares budgets has to run the same document under more than one
    /// limit inside one process.
    #[cfg(feature = "bench-support")]
    pub fn install_exact(cx: &mut App, limit: usize) {
        cx.set_global(MediaCoordinator::new(limit));
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn reserved(&self) -> usize {
        self.reserved.load(Ordering::Acquire)
    }

    /// Highest reservation this coordinator has held. A permit is live only
    /// while a job runs, so a sample taken between frames always reads zero;
    /// the high-water mark is what tells whether the ceiling was reached.
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::Acquire)
    }

    pub fn try_reserve(&self, bytes: usize) -> Option<BytePermit> {
        let bytes = bytes.min(self.limit);
        let mut current = self.reserved.load(Ordering::Acquire);
        loop {
            let next = current.saturating_add(bytes);
            if next > self.limit {
                return None;
            }
            match self.reserved.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.peak.fetch_max(next, Ordering::AcqRel);
                    return Some(BytePermit {
                        bytes,
                        reserved: Arc::clone(&self.reserved),
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Admit a job that estimates `bytes` of working memory. `None` means the
    /// budget is already held by other jobs, so the caller must not start the
    /// work: it should leave the job unqueued and try again on a later frame,
    /// which only happens once the permits in flight are returned.
    ///
    /// A single estimate is clamped to the limit, so one oversized raster is
    /// admitted on an idle budget instead of waiting for a room that never
    /// opens.
    pub fn admit(cx: &App, bytes: usize) -> Option<BytePermit> {
        cx.global::<MediaCoordinator>().try_reserve(bytes)
    }

    pub fn app_peak(cx: &App) -> usize {
        cx.global::<MediaCoordinator>().peak()
    }

    #[cfg(feature = "bench-support")]
    pub fn reset_peak(cx: &mut App) {
        if let Some(coordinator) = cx.try_global::<MediaCoordinator>() {
            coordinator.peak.store(0, Ordering::Release);
        }
    }
}

pub struct BytePermit {
    bytes: usize,
    reserved: Arc<AtomicUsize>,
}

impl BytePermit {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        self.reserved.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Every byte one view currently holds across its media caches, split by
/// account so a reader can tell which cache grew. Only resident bytes are
/// counted: a compressed source, a display raster, a mermaid raster, a math
/// raster and the zoom overlay's current image. Bytes held by a job that has
/// not delivered yet are counted by the coordinator, not here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub display_bytes: usize,
    pub display_entries: usize,
    pub source_bytes: usize,
    pub source_entries: usize,
    pub mermaid_bytes: usize,
    pub mermaid_entries: usize,
    pub math_bytes: usize,
    pub math_entries: usize,
    pub zoom_bytes: usize,
}

impl Account {
    pub fn media_bytes(&self) -> usize {
        self.display_bytes
            .saturating_add(self.source_bytes)
            .saturating_add(self.mermaid_bytes)
            .saturating_add(self.math_bytes)
            .saturating_add(self.zoom_bytes)
    }

    pub fn entries(&self) -> usize {
        self.display_entries
            .saturating_add(self.source_entries)
            .saturating_add(self.mermaid_entries)
            .saturating_add(self.math_entries)
    }

    pub fn merge(&mut self, other: Account) {
        self.display_bytes = self.display_bytes.saturating_add(other.display_bytes);
        self.display_entries = self.display_entries.saturating_add(other.display_entries);
        self.source_bytes = self.source_bytes.saturating_add(other.source_bytes);
        self.source_entries = self.source_entries.saturating_add(other.source_entries);
        self.mermaid_bytes = self.mermaid_bytes.saturating_add(other.mermaid_bytes);
        self.mermaid_entries = self.mermaid_entries.saturating_add(other.mermaid_entries);
        self.math_bytes = self.math_bytes.saturating_add(other.math_bytes);
        self.math_entries = self.math_entries.saturating_add(other.math_entries);
        self.zoom_bytes = self.zoom_bytes.saturating_add(other.zoom_bytes);
    }
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "media={} display={}/{} source={}/{} mermaid={}/{} math={}/{} zoom={}",
            self.media_bytes(),
            self.display_bytes,
            self.display_entries,
            self.source_bytes,
            self.source_entries,
            self.mermaid_bytes,
            self.mermaid_entries,
            self.math_bytes,
            self.math_entries,
            self.zoom_bytes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Account, MediaCoordinator, drop_image_deferred};
    use gpui::TestAppContext;
    use image::Frame;
    use smallvec::smallvec;
    use std::sync::Arc;

    #[gpui::test]
    fn evicted_media_releases_its_arc_only_after_the_deferred_flush(cx: &mut TestAppContext) {
        let image = Arc::new(gpui::RenderImage::new(smallvec![Frame::new(
            image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]))
        )]));
        let baseline = Arc::strong_count(&image);

        cx.update(|cx| {
            drop_image_deferred(cx, image.clone());
            assert_eq!(Arc::strong_count(&image), baseline + 1);
        });

        assert_eq!(Arc::strong_count(&image), baseline);
    }

    #[test]
    fn media_bytes_sums_every_account_once() {
        let account = Account {
            display_bytes: 10,
            source_bytes: 20,
            mermaid_bytes: 30,
            math_bytes: 40,
            zoom_bytes: 50,
            display_entries: 1,
            source_entries: 2,
            mermaid_entries: 3,
            math_entries: 4,
        };
        assert_eq!(account.media_bytes(), 150);
        assert_eq!(account.entries(), 10);
    }

    #[test]
    fn merging_two_accounts_adds_each_field() {
        let mut left = Account {
            display_bytes: 10,
            display_entries: 1,
            ..Default::default()
        };
        left.merge(Account {
            display_bytes: 5,
            display_entries: 2,
            mermaid_bytes: 7,
            mermaid_entries: 3,
            ..Default::default()
        });
        assert_eq!(left.display_bytes, 15);
        assert_eq!(left.display_entries, 3);
        assert_eq!(left.mermaid_bytes, 7);
        assert_eq!(left.mermaid_entries, 3);
    }

    #[test]
    fn a_permit_returns_its_bytes_only_when_it_is_dropped() {
        let coordinator = MediaCoordinator::new(100);
        let first = coordinator.try_reserve(60).expect("first job");
        assert_eq!(coordinator.reserved(), 60);
        assert!(
            coordinator.try_reserve(60).is_none(),
            "a second job that would exceed the budget must be refused"
        );
        let second = coordinator.try_reserve(40).expect("fills the budget");
        assert!(coordinator.try_reserve(1).is_none());
        drop(first);
        assert_eq!(coordinator.reserved(), 40);
        assert!(
            coordinator.try_reserve(61).is_none(),
            "only the released 60 bytes are free again"
        );
        let third = coordinator.try_reserve(60).expect("fits exactly");
        assert_eq!(coordinator.reserved(), 100);
        drop(second);
        drop(third);
        assert_eq!(coordinator.reserved(), 0);
    }

    #[test]
    fn one_estimate_above_the_budget_is_clamped_instead_of_waiting_forever() {
        let coordinator = MediaCoordinator::new(100);
        let permit = coordinator
            .try_reserve(4096)
            .expect("a single oversized raster must still be admitted on an idle budget");
        assert_eq!(permit.bytes(), 100);
        assert_eq!(coordinator.reserved(), 100);
        assert!(
            coordinator.try_reserve(1).is_none(),
            "the clamped estimate holds the whole budget while it runs"
        );
        drop(permit);
        assert_eq!(coordinator.reserved(), 0);
    }
}
