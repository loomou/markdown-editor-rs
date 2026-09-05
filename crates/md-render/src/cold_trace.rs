use std::sync::OnceLock;
use std::time::Instant;

static GPUI: OnceLock<Instant> = OnceLock::new();
static SINK: OnceLock<fn(&str)> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("MD_TEST_COLD_ONCE").is_some())
}

pub fn mark_gpui() {
    let _ = GPUI.set(Instant::now());
}

pub fn gpui_ms() -> f64 {
    GPUI.get()
        .map(|t| t.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub fn set_sink(sink: fn(&str)) {
    let _ = SINK.set(sink);
}

pub fn log(line: &str) {
    if enabled() {
        if let Some(sink) = SINK.get() {
            sink(line);
        } else {
            eprintln!("{line}");
        }
    }
}
