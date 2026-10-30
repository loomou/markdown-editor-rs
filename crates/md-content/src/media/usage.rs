use super::drop_image_deferred;
use crate::pixels::ReadyImage;
use gpui::{App, RenderImage, Window};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Which pixels each window actually drew, so a texture is only released once
/// nothing paints it any more. Cache eviction says a raster is no longer worth
/// keeping, not that it has left the screen: a frame that already painted it
/// would otherwise lose its texture and upload it again on the next frame.
///
/// A frame that is never painted keeps its previous set. GPUI reuses the last
/// draw when nothing invalidated, and reading that as "nothing is visible"
/// would release textures that are still on screen.
#[derive(Default)]
pub struct MediaUsage {
    frames: HashMap<u64, HashSet<u64>>,
    painting: HashMap<u64, HashSet<u64>>,
    pending: Vec<Pending>,
    released_after_wait: usize,
    released_immediately: usize,
}

struct Pending {
    id: u64,
    image: Arc<RenderImage>,
}

impl gpui::Global for MediaUsage {}

fn with_usage(cx: &mut App, f: impl FnOnce(&mut MediaUsage)) {
    if cx.try_global::<MediaUsage>().is_some() {
        f(cx.global_mut::<MediaUsage>());
    }
}

fn window_id(window: &Window) -> u64 {
    window.window_handle().window_id().as_u64()
}

impl MediaUsage {
    pub fn install(cx: &mut App) {
        if cx.try_global::<MediaUsage>().is_some() {
            return;
        }
        cx.set_global(MediaUsage::default());
        cx.on_window_closed(|cx, window_id| {
            with_usage(cx, |usage| {
                usage.frames.remove(&window_id.as_u64());
                usage.painting.remove(&window_id.as_u64());
            });
        })
        .detach();
    }

    pub fn begin_frame(cx: &mut App, window: &Window) {
        Self::begin_frame_id(cx, window_id(window));
    }

    fn begin_frame_id(cx: &mut App, window_id: u64) {
        with_usage(cx, |usage| {
            usage.painting.insert(window_id, HashSet::new());
        });
    }

    /// Record that this frame painted `image`. Images without a lease are not
    /// tracked: nothing admitted them, so there is no identity to wait on and
    /// they are released as soon as their cache entry goes.
    pub fn note(cx: &mut App, window: &Window, image: &ReadyImage) {
        Self::note_alloc(cx, window, image.allocation_id());
    }

    /// Same as `note` for a caller that only kept the allocation id, such as the
    /// zoom overlay which hands gpui a bare `RenderImage`.
    pub fn note_alloc(cx: &mut App, window: &Window, id: Option<u64>) {
        if let Some(id) = id {
            Self::note_id(cx, window_id(window), id);
        }
    }

    pub fn note_id(cx: &mut App, window_id: u64, id: u64) {
        with_usage(cx, |usage| {
            usage.painting.entry(window_id).or_default().insert(id);
        });
    }

    /// Finish the frame for this window and settle every release that was
    /// waiting to see whether anything still painted it. A pending release that
    /// reappears in the new set is cancelled by staying in the queue.
    pub fn commit_frame(cx: &mut App, window: &Window) {
        Self::commit_frame_id(cx, window_id(window));
    }

    fn commit_frame_id(cx: &mut App, window_id: u64) {
        if cx.try_global::<MediaUsage>().is_none() {
            return;
        }
        let drop_now = {
            let usage = cx.global_mut::<MediaUsage>();
            if let Some(set) = usage.painting.remove(&window_id) {
                usage.frames.insert(window_id, set);
            }
            let pending = std::mem::take(&mut usage.pending);
            let mut keep = Vec::with_capacity(pending.len());
            let mut drop_now = Vec::new();
            for entry in pending {
                if usage.in_use(entry.id) {
                    keep.push(entry);
                } else {
                    usage.released_after_wait = usage.released_after_wait.saturating_add(1);
                    drop_now.push(entry.image);
                }
            }
            usage.pending = keep;
            usage.released_immediately = usage.released_immediately.saturating_add(drop_now.len());
            drop_now
        };
        for image in drop_now {
            drop_image_deferred(cx, image);
        }
    }

    /// Ask for a texture to be released. `id` is the allocation the pixels came
    /// from; `None` means the raster was never admitted and has no identity to
    /// wait on.
    pub fn release(cx: &mut App, id: Option<u64>, image: Arc<RenderImage>) {
        let mut waiting = false;
        if let Some(id) = id
            && cx.try_global::<MediaUsage>().is_some()
        {
            let usage = cx.global_mut::<MediaUsage>();
            if usage.in_use(id) {
                usage.pending.push(Pending {
                    id,
                    image: image.clone(),
                });
                waiting = true;
            }
        }
        if !waiting {
            with_usage(cx, |usage| {
                usage.released_immediately = usage.released_immediately.saturating_add(1);
            });
            drop_image_deferred(cx, image);
        }
    }

    pub fn release_ready(cx: &mut App, image: ReadyImage) {
        let id = image.allocation_id();
        Self::release(cx, id, image.image);
    }

    fn in_use(&self, id: u64) -> bool {
        self.frames.values().any(|set| set.contains(&id))
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    pub fn tracked_windows(&self) -> usize {
        self.frames.len()
    }

    /// Releases that waited for a frame to stop painting them, and releases that
    /// went straight to the drop queue. A probe reads these to tell a texture
    /// that outlived its cache entry from one that was never on screen.
    pub fn released_after_wait(&self) -> usize {
        self.released_after_wait
    }

    pub fn released_immediately(&self) -> usize {
        self.released_immediately
    }

    #[cfg(test)]
    pub fn note_frame_for_test(&mut self, window_id: u64, ids: &[u64]) {
        self.frames.insert(window_id, ids.iter().copied().collect());
    }
}

#[cfg(test)]
mod tests {
    use super::MediaUsage;
    use gpui::TestAppContext;
    use image::Frame;
    use smallvec::smallvec;
    use std::sync::Arc;

    fn image() -> Arc<gpui::RenderImage> {
        Arc::new(gpui::RenderImage::new(smallvec![Frame::new(
            image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]))
        )]))
    }

    #[gpui::test]
    fn pixels_the_last_frame_painted_wait_for_a_frame_that_does_not(cx: &mut TestAppContext) {
        let image = image();
        let baseline = Arc::strong_count(&image);
        cx.update(|cx| {
            MediaUsage::install(cx);
            cx.global_mut::<MediaUsage>().note_frame_for_test(1, &[42]);

            MediaUsage::release(cx, Some(42), image.clone());
            assert_eq!(
                cx.global::<MediaUsage>().pending(),
                1,
                "the last frame painted these pixels, so the texture must wait"
            );
            assert_eq!(Arc::strong_count(&image), baseline + 1);

            MediaUsage::begin_frame_id(cx, 1);
            MediaUsage::note_id(cx, 1, 43);
            MediaUsage::commit_frame_id(cx, 1);

            assert_eq!(cx.global::<MediaUsage>().pending(), 0);
            assert_eq!(cx.global::<MediaUsage>().released_after_wait(), 1);
        });
        assert_eq!(
            Arc::strong_count(&image),
            baseline,
            "the settled release must leave the drop queue after the frame"
        );
    }

    #[gpui::test]
    fn pixels_a_frame_still_paints_survive_the_commit(cx: &mut TestAppContext) {
        cx.update(|cx| {
            MediaUsage::install(cx);
            let image = image();
            let baseline = Arc::strong_count(&image);
            cx.global_mut::<MediaUsage>().note_frame_for_test(1, &[7]);

            MediaUsage::release(cx, Some(7), image.clone());
            assert_eq!(cx.global::<MediaUsage>().pending(), 1);

            MediaUsage::begin_frame_id(cx, 1);
            MediaUsage::note_id(cx, 1, 7);
            MediaUsage::commit_frame_id(cx, 1);

            assert_eq!(
                cx.global::<MediaUsage>().pending(),
                1,
                "the new frame paints it again, so the release is cancelled"
            );
            assert_eq!(cx.global::<MediaUsage>().released_after_wait(), 0);
            assert_eq!(Arc::strong_count(&image), baseline + 1);
        });
    }

    #[gpui::test]
    fn pixels_no_frame_painted_go_straight_to_the_drop_queue(cx: &mut TestAppContext) {
        let image = image();
        let baseline = Arc::strong_count(&image);
        cx.update(|cx| {
            MediaUsage::install(cx);
            cx.global_mut::<MediaUsage>().note_frame_for_test(1, &[1]);

            MediaUsage::release(cx, Some(99), image.clone());
            assert_eq!(cx.global::<MediaUsage>().pending(), 0);
            assert_eq!(cx.global::<MediaUsage>().released_immediately(), 1);
            assert_eq!(
                Arc::strong_count(&image),
                baseline + 1,
                "the deferred queue holds the texture until the frame ends"
            );
        });
        assert_eq!(
            Arc::strong_count(&image),
            baseline,
            "the deferred flush must not leave the drop queue holding a texture"
        );
    }

    #[gpui::test]
    fn a_release_without_an_allocation_is_never_queued(cx: &mut TestAppContext) {
        cx.update(|cx| {
            MediaUsage::install(cx);
            cx.global_mut::<MediaUsage>().note_frame_for_test(1, &[5]);
            let image = image();
            MediaUsage::release(cx, None, image);
            assert_eq!(cx.global::<MediaUsage>().pending(), 0);
            assert_eq!(cx.global::<MediaUsage>().released_immediately(), 1);
        });
    }

    #[gpui::test]
    fn a_window_that_closes_stops_keeping_its_pixels_alive(cx: &mut TestAppContext) {
        cx.update(|cx| {
            MediaUsage::install(cx);
            let usage = cx.global_mut::<MediaUsage>();
            usage.note_frame_for_test(1, &[11]);
            usage.note_frame_for_test(2, &[12]);
            usage.frames.remove(&1);
            assert!(!cx.global::<MediaUsage>().in_use(11));
            assert!(cx.global::<MediaUsage>().in_use(12));
        });
    }
}
