use gpui::{Context, Task};
use std::time::Duration;

pub struct Blink {
    visible: bool,

    wake: bool,

    live: bool,
    task: Option<Task<()>>,
}

impl Default for Blink {
    fn default() -> Self {
        Self {
            visible: true,
            wake: false,
            live: false,
            task: None,
        }
    }
}

impl Blink {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn set_live(&mut self, live: bool) {
        self.live = live;
    }

    #[doc(hidden)]
    pub fn live(&self) -> bool {
        self.live
    }

    pub fn wake(&mut self) {
        self.visible = true;
        self.wake = true;
    }

    pub fn stop(&mut self) {
        self.task = None;
        self.visible = true;
        self.wake = false;
    }

    pub fn start<T: 'static>(
        &mut self,
        period: Duration,
        cx: &mut Context<'_, T>,
        pick: impl Fn(&mut T) -> Option<&mut Blink> + 'static,
    ) {
        self.stop();
        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(period).await;
                let alive = this
                    .update(cx, |host, cx| match pick(host) {
                        Some(blink) => {
                            if blink.tick() {
                                cx.notify();
                            }
                            true
                        }
                        None => false,
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        }));
    }

    fn tick(&mut self) -> bool {
        let wake = std::mem::take(&mut self.wake);
        let next = wake || !self.live || !self.visible;
        let changed = next != self.visible;
        self.visible = next;

        changed && self.live
    }
}

#[cfg(test)]
mod tests {
    use super::Blink;

    fn live() -> Blink {
        let mut b = Blink::default();
        b.set_live(true);
        b
    }

    #[test]
    fn a_live_caret_alternates_and_asks_for_a_repaint() {
        let mut b = live();
        assert!(b.visible());
        assert!(b.tick(), "a bright-to-dim transition should repaint");
        assert!(!b.visible());
        assert!(b.tick(), "a dim-to-bright transition should also repaint");
        assert!(b.visible());
    }

    #[test]
    fn waking_eats_one_beat_so_typing_never_dims_the_caret() {
        let mut b = live();
        assert!(b.tick());
        assert!(
            !b.visible(),
            "it should have dropped to the dim phase first"
        );
        b.wake();
        assert!(b.visible(), "waking should snap right back to bright");
        assert!(
            !b.tick(),
            "the very next beat is eaten: phase unchanged, so no repaint"
        );
        assert!(b.visible());
        assert!(b.tick(), "only the beat after that may turn it dim");
        assert!(!b.visible());
    }

    #[test]
    fn an_unfocused_caret_parks_bright_without_repainting() {
        let mut b = live();
        assert!(b.tick());
        assert!(!b.visible());
        b.set_live(false);
        assert!(!b.tick(), "losing focus should not repaint");
        assert!(
            b.visible(),
            "but it should stay bright so the frame in which focus returns is solid"
        );
        assert!(
            !b.tick(),
            "every beat after that is wasted and must not repaint"
        );
        assert!(b.visible());
    }
}
