use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{App, AsyncWindowContext, WeakEntity, Window};

use crate::Shell;

const ENV: &str = "MARKDOWN_EDITOR_RS_ZOOM_ONCE";
const SCROLL_ENV: &str = "MARKDOWN_EDITOR_RS_SCROLL_ONCE";
const START_DELAY: Duration = Duration::from_millis(1500);
const SETTLE: Duration = Duration::from_millis(400);
const POLL: Duration = Duration::from_micros(500);
const STEP_PX: f64 = 600.0;

pub fn spawn(window: &mut Window, shell: WeakEntity<Shell>, cx: &mut App) {
    let zoom = std::env::var_os(ENV).is_some();
    let scroll = std::env::var(SCROLL_ENV)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok());
    if !zoom && scroll.is_none() {
        return;
    }
    let wcx = window.to_async(cx);
    wcx.spawn(async move |wcx: &mut AsyncWindowContext| {
        wcx.background_executor().timer(START_DELAY).await;
        if zoom {
            let _ = wcx.update(|_, cx| {
                if let Some(shell) = shell.upgrade() {
                    shell.update(cx, |s, cx| s.bench_open_first_mermaid(cx));
                }
            });
            wcx.background_executor().timer(SETTLE).await;
            let _ = wcx.update(|_, cx| cx.quit());
            return;
        }
        let steps = scroll.unwrap_or(0);
        for step in 0..steps {
            let _ = wcx.update(|_, cx| {
                if let Some(shell) = shell.upgrade() {
                    shell.update(cx, |s, cx| {
                        s.bench_scroll_to(step as f64 * STEP_PX, cx);
                    });
                }
            });
            next_frame(wcx).await;
        }
        wcx.background_executor()
            .timer(Duration::from_millis(3000))
            .await;
        let _ = wcx.update(|_, cx| cx.quit());
    })
    .detach();
}

async fn next_frame(wcx: &mut AsyncWindowContext) {
    let seen = Rc::new(Cell::new(false));
    let flag = Rc::clone(&seen);
    wcx.on_next_frame(move |_, _| flag.set(true));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !seen.get() {
        if Instant::now() >= deadline {
            return;
        }
        wcx.background_executor().timer(POLL).await;
    }
}
