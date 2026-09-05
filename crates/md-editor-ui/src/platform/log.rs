use std::fs;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub struct Guard {
    _file: Option<tracing_appender::non_blocking::WorkerGuard>,
}

fn emit_cold(line: &str) {
    tracing::info!(target: "md_editor::cold", "{line}");
}

fn log_dir() -> Option<std::path::PathBuf> {
    crate::platform::paths::log_dir()
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_env("MD_TEST_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("warn,md_editor=info"))
}

fn panic_payload(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "Box<dyn Any>".into()
    }
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info
            .location()
            .map(|l| l.to_string())
            .unwrap_or_else(|| "unknown".into());
        let payload = panic_payload(info);
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(location = %loc, backtrace = %backtrace, "panic: {payload}");
        default_hook(info);
    }));
}

pub fn init() -> Guard {
    md_render::cold_trace::set_sink(emit_cold);
    if tracing::dispatcher::has_been_set() {
        return Guard { _file: None };
    }

    let filter = env_filter();
    let (file_writer, file_guard) = match log_dir().and_then(|dir| {
        fs::create_dir_all(&dir).ok()?;
        let appender = tracing_appender::rolling::never(dir, "md-test.log");
        Some(tracing_appender::non_blocking(appender))
    }) {
        Some((writer, guard)) => (Some(writer), Some(guard)),
        None => (None, None),
    };
    let file_layer = file_writer.map(|writer| {
        fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(writer)
    });
    let stderr_layer = (cfg!(debug_assertions) || file_layer.is_none())
        .then(|| fmt::layer().with_writer(std::io::stderr));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init();
    install_panic_hook();
    Guard { _file: file_guard }
}
