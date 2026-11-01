use gpui::{ImageFormat, RenderImage};
use image::AnimationDecoder;
use image::metadata::LoopCount;
use smallvec::SmallVec;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub const MIN_FRAME_DELAY: Duration = Duration::from_millis(10);
pub const DEFAULT_FRAME_DELAY: Duration = Duration::from_millis(100);

pub struct Animation {
    delays: Vec<Duration>,
    total: Duration,
    plays: Option<Duration>,
    frame: AtomicUsize,
    started: Instant,
}

impl Animation {
    pub fn new(delays: Vec<Duration>, loops: Option<u32>) -> Self {
        let delays: Vec<Duration> = delays
            .into_iter()
            .map(|delay| delay.max(MIN_FRAME_DELAY))
            .collect();
        let total: Duration = delays.iter().sum();
        let plays = loops.and_then(|n| total.checked_mul(n));
        Animation {
            delays,
            total,
            plays,
            frame: AtomicUsize::new(0),
            started: Instant::now(),
        }
    }

    pub fn frame_count(&self) -> usize {
        self.delays.len()
    }

    pub fn current(&self) -> usize {
        self.frame.load(Ordering::Relaxed)
    }

    pub fn frame_at(&self, elapsed: Duration) -> usize {
        if self.delays.len() <= 1 || self.total.is_zero() {
            return 0;
        }
        if self.finished_at(elapsed) {
            return self.delays.len() - 1;
        }
        let mut offset = elapsed.as_nanos() % self.total.as_nanos();
        for (index, delay) in self.delays.iter().enumerate() {
            let step = delay.as_nanos();
            if offset < step {
                return index;
            }
            offset -= step;
        }
        self.delays.len() - 1
    }

    pub fn tick(&self) -> bool {
        let next = self.frame_at(self.started.elapsed());
        self.frame.swap(next, Ordering::Relaxed) != next
    }

    pub fn until_next(&self) -> Duration {
        if self.delays.len() <= 1 {
            return Duration::ZERO;
        }
        let elapsed = self.started.elapsed();
        if self.finished_at(elapsed) {
            return Duration::ZERO;
        }
        let offset = elapsed.as_nanos() % self.total.as_nanos();
        let mut boundary = 0u128;
        for delay in &self.delays {
            boundary += delay.as_nanos();
            if offset < boundary {
                let remaining = boundary - offset;
                return Duration::from_nanos(remaining.min(u64::MAX as u128) as u64);
            }
        }
        Duration::ZERO
    }

    pub fn finished_at(&self, elapsed: Duration) -> bool {
        self.plays.is_some_and(|plays| elapsed >= plays)
    }

    pub fn finished(&self) -> bool {
        self.finished_at(self.started.elapsed())
    }
}

pub fn image_from_frames(frames: Vec<image::Frame>) -> Arc<RenderImage> {
    let mut frames = frames;
    for frame in &mut frames {
        for pixel in frame.buffer_mut().chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    let frames: SmallVec<[image::Frame; 1]> = frames.into_iter().collect();
    Arc::new(RenderImage::new(frames))
}

pub fn delays_of(frames: &[image::Frame]) -> Vec<Duration> {
    frames
        .iter()
        .map(|frame| {
            let (numer, denom) = frame.delay().numer_denom_ms();
            if denom == 0 || numer == 0 {
                DEFAULT_FRAME_DELAY
            } else {
                Duration::from_millis(u64::from(numer / denom))
            }
        })
        .collect()
}

fn loops_of(count: LoopCount) -> Option<u32> {
    match count {
        LoopCount::Infinite => None,
        LoopCount::Finite(n) => Some(n.get()),
    }
}

pub fn decode_animated(
    format: ImageFormat,
    bytes: &[u8],
) -> Option<(Arc<RenderImage>, Vec<Duration>, Option<u32>)> {
    let (frames, loops) = match format {
        ImageFormat::Gif => {
            let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
            let loops = loops_of(decoder.loop_count());
            (decoder.into_frames().collect_frames().ok()?, loops)
        }
        ImageFormat::Png => {
            let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
                .ok()?
                .apng()
                .ok()?;
            let loops = loops_of(decoder.loop_count());
            (decoder.into_frames().collect_frames().ok()?, loops)
        }
        ImageFormat::Webp => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
            let loops = loops_of(decoder.loop_count());
            (decoder.into_frames().collect_frames().ok()?, loops)
        }
        _ => return None,
    };
    if frames.len() <= 1 {
        return None;
    }
    let delays = delays_of(&frames);
    Some((image_from_frames(frames), delays, loops))
}

#[cfg(test)]
mod tests {
    use super::{Animation, DEFAULT_FRAME_DELAY, MIN_FRAME_DELAY, delays_of};
    use std::time::Duration;

    fn animation(delays: &[u64], loops: Option<u32>) -> Animation {
        Animation::new(
            delays.iter().map(|ms| Duration::from_millis(*ms)).collect(),
            loops,
        )
    }

    #[test]
    fn a_frame_lasts_as_long_as_its_delay() {
        let a = animation(&[100, 200, 50], None);
        assert_eq!(a.frame_at(Duration::ZERO), 0);
        assert_eq!(a.frame_at(Duration::from_millis(99)), 0);
        assert_eq!(a.frame_at(Duration::from_millis(100)), 1);
        assert_eq!(a.frame_at(Duration::from_millis(299)), 1);
        assert_eq!(a.frame_at(Duration::from_millis(300)), 2);
    }

    #[test]
    fn the_cycle_wraps_until_the_loop_count_runs_out() {
        let a = animation(&[100, 100], None);
        assert_eq!(a.frame_at(Duration::from_millis(200)), 0);
        assert_eq!(a.frame_at(Duration::from_millis(350)), 1);

        let finite = animation(&[100, 100], Some(2));
        assert_eq!(finite.frame_at(Duration::from_millis(399)), 1);
        assert_eq!(
            finite.frame_at(Duration::from_millis(400)),
            1,
            "a finished animation holds its last frame"
        );
        assert!(!finite.finished_at(Duration::from_millis(399)));
        assert!(finite.finished_at(Duration::from_millis(400)));
        assert!(!a.finished_at(Duration::from_secs(86_400)));
    }

    #[test]
    fn a_zero_delay_falls_back_to_the_default() {
        let frames = [
            image::Frame::from_parts(
                image::RgbaImage::new(1, 1),
                0,
                0,
                image::Delay::from_numer_denom_ms(0, 1),
            ),
            image::Frame::from_parts(
                image::RgbaImage::new(1, 1),
                0,
                0,
                image::Delay::from_numer_denom_ms(40, 1),
            ),
        ];
        assert_eq!(
            delays_of(&frames),
            vec![DEFAULT_FRAME_DELAY, Duration::from_millis(40)]
        );
    }

    #[test]
    fn a_delay_shorter_than_the_minimum_tick_is_floored() {
        let a = Animation::new(vec![Duration::ZERO, Duration::ZERO], None);
        assert_eq!(a.frame_at(Duration::from_millis(1)), 0);
        assert_eq!(a.frame_at(MIN_FRAME_DELAY), 1);
    }

    #[test]
    fn the_next_frame_never_reports_more_than_its_delay() {
        let a = animation(&[100, 250], None);
        let wait = a.until_next();
        assert!(wait <= Duration::from_millis(100), "wait was {wait:?}");
        let single = animation(&[100], None);
        assert_eq!(single.until_next(), Duration::ZERO);
    }

    #[test]
    fn ticking_only_reports_a_change_when_the_frame_moves() {
        let a = Animation::new(vec![Duration::from_secs(60), Duration::from_secs(60)], None);
        assert!(!a.tick(), "still on the first frame");
        assert_eq!(a.current(), 0);
    }
}
