use gpui::{App, RenderImage};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

mod usage;
mod warm;

pub use usage::MediaUsage;
pub use warm::{
    COOL_GRACE, COOL_MIN_GAIN_PERCENT, DEFAULT_WARM_CODEC, WarmCodec, WarmImage, decode, decode_raw,
    encode, encode_raw, estimate_decode_job, estimate_encode_job,
};

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

/// Ceiling on the pixels every cache in the app may keep at once. The per-cache
/// limits are slices of this, so a cache cannot grow into headroom another one
/// already spent. Set above what the caches held together before the split
/// existed, so enabling the ceiling does not by itself force eviction; what
/// this step changes is who owns the total, not how much it is.
pub const DEFAULT_RESIDENT_BYTES: usize = 256 * 1024 * 1024;

/// Screens of RGBA the resident ceiling is worth. A window keeps roughly this
/// much decoded media around, so the ceiling follows the viewport instead of
/// being one number that is generous on a laptop and cramped on a 4K display.
const RESIDENT_SCREENS: usize = 40;
const MIN_RESIDENT_BYTES: usize = 192 * 1024 * 1024;
const MAX_RESIDENT_BYTES: usize = 512 * 1024 * 1024;

pub fn resident_ceiling(screen_bytes: usize) -> usize {
    screen_bytes
        .saturating_mul(RESIDENT_SCREENS)
        .clamp(MIN_RESIDENT_BYTES, MAX_RESIDENT_BYTES)
}

/// Which queue a refused job came from. A refusal is not a failure: the job
/// stays unqueued and is retried on a later frame, so these counts say whether
/// a budget is too small for the work a document asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Source,
    Display,
    Zoom,
    Mermaid,
    Math,
    /// Compressing an off-screen raster into its warm representation.
    Cool,
    /// Inflating a warm entry back into pixels.
    Thaw,
}

impl JobKind {
    pub const ALL: [JobKind; 7] = [
        JobKind::Source,
        JobKind::Display,
        JobKind::Zoom,
        JobKind::Mermaid,
        JobKind::Math,
        JobKind::Cool,
        JobKind::Thaw,
    ];

    pub fn index(self) -> usize {
        match self {
            JobKind::Source => 0,
            JobKind::Display => 1,
            JobKind::Zoom => 2,
            JobKind::Mermaid => 3,
            JobKind::Math => 4,
            JobKind::Cool => 5,
            JobKind::Thaw => 6,
        }
    }
}

/// How the resident ceiling is divided. The order matters more than the exact
/// numbers: sources are what makes a re-visit cheap, so they get the largest
/// slice; display pixels are rebuilt from those sources; vector rasters are the
/// costliest to rebuild but the least numerous; formulas are small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResidentSplit {
    pub display: usize,
    pub source: usize,
    pub mermaid: usize,
    pub math: usize,
}

pub fn split_resident(total: usize) -> ResidentSplit {
    let share = |percent: usize| total / 100 * percent;
    ResidentSplit {
        display: share(30),
        source: share(40),
        mermaid: share(20),
        math: share(8),
    }
}

struct Ledger {
    job_limit: usize,
    job_reserved: AtomicUsize,
    job_peak: AtomicUsize,
    resident_limit: AtomicUsize,
    resident: AtomicUsize,
    resident_peak: AtomicUsize,
    next_lease_id: AtomicUsize,
    refused: [AtomicUsize; 7],
    clamped: AtomicUsize,
}

impl Ledger {
    fn new(job_limit: usize, resident_limit: usize) -> Self {
        Ledger {
            job_limit,
            job_reserved: AtomicUsize::new(0),
            job_peak: AtomicUsize::new(0),
            resident_limit: AtomicUsize::new(resident_limit),
            resident: AtomicUsize::new(0),
            resident_peak: AtomicUsize::new(0),
            next_lease_id: AtomicUsize::new(1),
            refused: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
            clamped: AtomicUsize::new(0),
        }
    }

    fn note_resident(&self, bytes: usize) {
        let next = self.resident.fetch_add(bytes, Ordering::AcqRel) + bytes;
        self.resident_peak.fetch_max(next, Ordering::AcqRel);
    }
}

pub struct MediaCoordinator {
    ledger: Arc<Ledger>,
}

impl gpui::Global for MediaCoordinator {}

impl MediaCoordinator {
    pub fn new(limit: usize) -> Self {
        MediaCoordinator {
            ledger: Arc::new(Ledger::new(limit, DEFAULT_RESIDENT_BYTES)),
        }
    }

    pub fn install(cx: &mut App, limit: usize) {
        if cx.try_global::<MediaCoordinator>().is_none() {
            cx.set_global(MediaCoordinator::new(limit));
        }
    }

    /// Install both ceilings explicitly. A test that wants to see the resident
    /// pressure path has to make the ceiling small enough to reach.
    pub fn install_with(cx: &mut App, job_limit: usize, resident_limit: usize) {
        cx.set_global(MediaCoordinator {
            ledger: Arc::new(Ledger::new(job_limit, resident_limit)),
        });
    }

    /// Replace the coordinator instead of installing it once. A probe that
    /// compares budgets has to run the same document under more than one
    /// limit inside one process.
    #[cfg(feature = "bench-support")]
    pub fn install_exact(cx: &mut App, limit: usize) {
        cx.set_global(MediaCoordinator::new(limit));
    }

    #[cfg(feature = "bench-support")]
    pub fn install_with_resident(cx: &mut App, job_limit: usize, resident_limit: usize) {
        cx.set_global(MediaCoordinator {
            ledger: Arc::new(Ledger::new(job_limit, resident_limit)),
        });
    }

    pub fn limit(&self) -> usize {
        self.ledger.job_limit
    }

    pub fn reserved(&self) -> usize {
        self.ledger.job_reserved.load(Ordering::Acquire)
    }

    /// Highest reservation this coordinator has held. A permit is live only
    /// while a job runs, so a sample taken between frames always reads zero;
    /// the high-water mark is what tells whether the ceiling was reached.
    pub fn peak(&self) -> usize {
        self.ledger.job_peak.load(Ordering::Acquire)
    }

    pub fn resident(&self) -> usize {
        self.ledger.resident.load(Ordering::Acquire)
    }

    pub fn resident_peak(&self) -> usize {
        self.ledger.resident_peak.load(Ordering::Acquire)
    }

    pub fn resident_limit(&self) -> usize {
        self.ledger.resident_limit.load(Ordering::Acquire)
    }

    /// Follow the viewport. A window that grows gets a larger ceiling; one that
    /// shrinks trims on its next eviction rather than holding the old one.
    pub fn set_resident_limit(cx: &mut App, limit: usize) {
        if let Some(coordinator) = cx.try_global::<MediaCoordinator>() {
            coordinator
                .ledger
                .resident_limit
                .store(limit, Ordering::Release);
        }
    }

    pub fn refusals(&self) -> [usize; 7] {
        let ledger = &self.ledger;
        [
            ledger.refused[0].load(Ordering::Acquire),
            ledger.refused[1].load(Ordering::Acquire),
            ledger.refused[2].load(Ordering::Acquire),
            ledger.refused[3].load(Ordering::Acquire),
            ledger.refused[4].load(Ordering::Acquire),
            ledger.refused[5].load(Ordering::Acquire),
            ledger.refused[6].load(Ordering::Acquire),
        ]
    }

    pub fn clamped(&self) -> usize {
        self.ledger.clamped.load(Ordering::Acquire)
    }

    pub fn try_reserve(&self, bytes: usize) -> Option<BytePermit> {
        let ledger = &self.ledger;
        let bytes = if bytes > ledger.job_limit {
            ledger.clamped.fetch_add(1, Ordering::AcqRel);
            ledger.job_limit
        } else {
            bytes
        };
        let mut current = ledger.job_reserved.load(Ordering::Acquire);
        loop {
            let next = current.saturating_add(bytes);
            if next > ledger.job_limit {
                return None;
            }
            match ledger.job_reserved.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    ledger.job_peak.fetch_max(next, Ordering::AcqRel);
                    return Some(BytePermit {
                        bytes,
                        ledger: Arc::clone(ledger),
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
    /// opens. Without a coordinator installed the work is admitted unmetered
    /// rather than blocked: a library user that never built a view still gets
    /// its images.
    pub fn admit(cx: &App, kind: JobKind, bytes: usize) -> Option<BytePermit> {
        let Some(coordinator) = cx.try_global::<MediaCoordinator>() else {
            return Some(BytePermit {
                bytes: 0,
                ledger: Arc::new(Ledger::new(0, 0)),
            });
        };
        match coordinator.try_reserve(bytes) {
            Some(permit) => Some(permit),
            None => {
                coordinator.ledger.refused[kind.index()].fetch_add(1, Ordering::AcqRel);
                None
            }
        }
    }

    pub fn app_peak(cx: &App) -> usize {
        cx.try_global::<MediaCoordinator>()
            .map_or(0, MediaCoordinator::peak)
    }

    pub fn app_resident(cx: &App) -> usize {
        cx.try_global::<MediaCoordinator>()
            .map_or(0, MediaCoordinator::resident)
    }

    pub fn app_resident_limit(cx: &App) -> usize {
        cx.try_global::<MediaCoordinator>()
            .map_or(0, MediaCoordinator::resident_limit)
    }

    #[cfg(feature = "bench-support")]
    pub fn reset_peak(cx: &mut App) {
        if let Some(coordinator) = cx.try_global::<MediaCoordinator>() {
            coordinator.ledger.job_peak.store(0, Ordering::Release);
            coordinator.ledger.resident_peak.store(0, Ordering::Release);
            for counter in &coordinator.ledger.refused {
                counter.store(0, Ordering::Release);
            }
            coordinator.ledger.clamped.store(0, Ordering::Release);
        }
    }
}

/// True once the caches together hold more pixels than the app may keep. A
/// cache that sees this keeps evicting past its own slice, so the overage lands
/// on whichever cache is trimming rather than on the one that grew last.
static COOLING: AtomicBool = AtomicBool::new(true);

/// Turn the warm cache off for an A/B run. The plan asks for a switch per
/// strategy so a measurement can attribute a change to one of them; a build
/// that ships does not touch this.
#[cfg(feature = "bench-support")]
pub fn set_cooling(enabled: bool) {
    COOLING.store(enabled, Ordering::Release);
}

pub fn cooling_enabled() -> bool {
    COOLING.load(Ordering::Acquire)
}

pub fn over_resident(cx: &App) -> bool {
    match cx.try_global::<MediaCoordinator>() {
        Some(coordinator) => coordinator.resident() > coordinator.resident_limit(),
        None => false,
    }
}

pub struct BytePermit {
    bytes: usize,
    ledger: Arc<Ledger>,
}

impl BytePermit {
    /// Bytes still held against the job budget. It shrinks when part of the
    /// estimate is converted into a resident lease on delivery.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Turn the delivered pixels into resident bytes and return the rest of the
    /// estimate to the job budget. Charging the output before releasing the
    /// scratch keeps the total from dipping below what is actually live, and the
    /// lease lives as long as the last clone of the image does, so a snapshot
    /// that outlives its cache entry stays paid for.
    pub fn into_lease(mut self, resident: usize) -> Arc<ByteLease> {
        let resident = resident.min(self.bytes);
        self.bytes = self.bytes.saturating_sub(resident);
        if resident > 0 {
            self.ledger.note_resident(resident);
            self.ledger
                .job_reserved
                .fetch_sub(resident, Ordering::AcqRel);
        }
        Arc::new(ByteLease {
            id: self
                .ledger
                .next_lease_id
                .fetch_add(1, Ordering::AcqRel) as u64,
            bytes: resident,
            ledger: Arc::clone(&self.ledger),
        })
    }
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        self.ledger
            .job_reserved
            .fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub struct ByteLease {
    id: u64,
    bytes: usize,
    ledger: Arc<Ledger>,
}

impl std::fmt::Debug for ByteLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteLease")
            .field("id", &self.id)
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl ByteLease {
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Stable for the life of the allocation and shared by every clone, so a
    /// frame that records which textures it drew can name one allocation
    /// without depending on pointer identity.
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for ByteLease {
    fn drop(&mut self) {
        self.ledger.resident.fetch_sub(self.bytes, Ordering::AcqRel);
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
    /// Compressed pixels kept for entries that are off-screen. Counted apart
    /// from the hot accounts so a reader can see how much of the total is
    /// content rather than width.
    pub warm_bytes: usize,
    pub warm_entries: usize,
    /// Entries whose compressed form did not pay for itself. They stay hot and
    /// are not encoded again, so this is also the count of encodes that were
    /// spent to learn that.
    pub warm_refused: usize,
    pub resident_bytes: usize,
    pub resident_limit: usize,
    pub refused_source: usize,
    pub refused_display: usize,
    pub refused_zoom: usize,
    pub refused_mermaid: usize,
    pub refused_math: usize,
    pub refused_cool: usize,
    pub refused_thaw: usize,
    pub clamped: usize,
    pub gpu_pending: usize,
    pub gpu_released_after_wait: usize,
    pub gpu_released_immediately: usize,
}

impl Account {
    pub fn media_bytes(&self) -> usize {
        self.display_bytes
            .saturating_add(self.source_bytes)
            .saturating_add(self.mermaid_bytes)
            .saturating_add(self.math_bytes)
            .saturating_add(self.zoom_bytes)
            .saturating_add(self.warm_bytes)
    }

    pub fn entries(&self) -> usize {
        self.display_entries
            .saturating_add(self.source_entries)
            .saturating_add(self.mermaid_entries)
            .saturating_add(self.math_entries)
    }

    pub fn refused(&self) -> usize {
        self.refused_source
            .saturating_add(self.refused_display)
            .saturating_add(self.refused_zoom)
            .saturating_add(self.refused_mermaid)
            .saturating_add(self.refused_math)
            .saturating_add(self.refused_cool)
            .saturating_add(self.refused_thaw)
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
        self.warm_bytes = self.warm_bytes.saturating_add(other.warm_bytes);
        self.warm_entries = self.warm_entries.saturating_add(other.warm_entries);
        self.warm_refused = self.warm_refused.saturating_add(other.warm_refused);
    }
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "media={} display={}/{} source={}/{} mermaid={}/{} math={}/{} zoom={} \
             warm={}/{} resident={}/{} refused={} clamped={} gpu-pending={} gpu-waited={} \
             gpu-direct={}",
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
            self.warm_bytes,
            self.warm_entries,
            self.resident_bytes,
            self.resident_limit,
            self.refused(),
            self.clamped,
            self.gpu_pending,
            self.gpu_released_after_wait,
            self.gpu_released_immediately,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Account, JobKind, MediaCoordinator, split_resident, drop_image_deferred,
    };
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
            ..Account::default()
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
        assert_eq!(coordinator.clamped(), 1);
        assert!(
            coordinator.try_reserve(1).is_none(),
            "the clamped estimate holds the whole budget while it runs"
        );
        drop(permit);
        assert_eq!(coordinator.reserved(), 0);
    }

    #[test]
    fn a_delivered_raster_moves_its_bytes_from_the_job_budget_to_the_resident_one() {
        let coordinator = MediaCoordinator::new(1000);
        let permit = coordinator.try_reserve(600).expect("job");
        let lease = permit.into_lease(250);
        assert_eq!(coordinator.reserved(), 0);
        assert_eq!(coordinator.resident(), 250);
        assert_eq!(lease.bytes(), 250);
        drop(lease);
        assert_eq!(coordinator.resident(), 0);
    }

    #[test]
    fn a_lease_never_charges_more_than_the_job_reserved() {
        let coordinator = MediaCoordinator::new(1000);
        let permit = coordinator.try_reserve(80).expect("job");
        let lease = permit.into_lease(500);
        assert_eq!(lease.bytes(), 80);
        assert_eq!(coordinator.resident(), 80);
        assert_eq!(coordinator.reserved(), 0);
    }

    #[test]
    fn shared_clones_of_a_lease_are_charged_once_until_the_last_one_goes() {
        let coordinator = MediaCoordinator::new(1000);
        let lease = coordinator.try_reserve(400).expect("job").into_lease(300);
        let shared = Arc::clone(&lease);
        assert_eq!(coordinator.resident(), 300);
        drop(lease);
        assert_eq!(
            coordinator.resident(),
            300,
            "a snapshot still holding the pixels must keep them charged"
        );
        drop(shared);
        assert_eq!(coordinator.resident(), 0);
    }

    #[gpui::test]
    fn a_refused_job_is_counted_against_the_queue_that_asked(cx: &mut TestAppContext) {
        cx.update(|cx| {
            MediaCoordinator::install(cx, 100);
            let held = MediaCoordinator::admit(cx, JobKind::Display, 100).expect("first job");
            assert!(
                MediaCoordinator::admit(cx, JobKind::Mermaid, 10).is_none(),
                "the budget is already held"
            );
            let refusals = cx.global::<MediaCoordinator>().refusals();
            assert_eq!(refusals[JobKind::Display.index()], 0);
            assert_eq!(refusals[JobKind::Mermaid.index()], 1);
            drop(held);
        });
    }

    #[test]
    fn a_lease_carries_an_id_that_every_clone_shares_and_a_new_job_does_not() {
        let coordinator = MediaCoordinator::new(1000);
        let first = coordinator.try_reserve(200).expect("job").into_lease(100);
        let second = coordinator.try_reserve(200).expect("job").into_lease(100);
        assert_eq!(first.id(), Arc::clone(&first).id());
        assert_ne!(first.id(), second.id());
    }

    #[test]
    fn a_permit_in_flight_on_another_thread_stays_charged_after_the_caller_gives_up() {
        let coordinator = MediaCoordinator::new(100);
        let permit = coordinator.try_reserve(50).expect("job");
        let worker = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(permit);
        });
        assert_eq!(
            coordinator.reserved(),
            50,
            "dropping the waiting side must not return the worker's permit"
        );
        worker.join().expect("worker");
        assert_eq!(coordinator.reserved(), 0);
    }

    #[test]
    fn the_resident_ceiling_splits_into_slices_that_sum_below_it() {
        let split = split_resident(1000);
        assert_eq!(split.display, 300);
        assert_eq!(split.source, 400);
        assert_eq!(split.mermaid, 200);
        assert_eq!(split.math, 80);
        assert!(split.display + split.source + split.mermaid + split.math <= 1000);
    }
}
