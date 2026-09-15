use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const KEEP_SESSIONS: usize = 10;

pub struct Guard {
    _file: Option<tracing_appender::non_blocking::WorkerGuard>,
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn session_file_name(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let tod = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}Z.log",
        tod / 3600,
        tod % 3600 / 60,
        tod % 60
    )
}

fn prune_old_logs(dir: &Path, live: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, e.path()))
        })
        .filter(|(_, p)| p != live)
        .collect();
    if logs.len() <= KEEP_SESSIONS {
        return;
    }
    logs.sort();
    let excess = logs.len() - KEEP_SESSIONS;
    for (_, p) in &logs[..excess] {
        let _ = fs::remove_file(p);
    }
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
        let path = dir.join(session_file_name(SystemTime::now()));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        prune_old_logs(&dir, &path);
        Some(file)
    }) {
        Some(file) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            (Some(writer), Some(guard))
        }
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
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "md-test starting");
    Guard { _file: file_guard }
}

#[cfg(test)]
mod tests {
    use super::{KEEP_SESSIONS, civil_from_days, prune_old_logs, session_file_name};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, UNIX_EPOCH};

    fn temp_log_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("md-test-log-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_711), (2026, 9, 15));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn session_file_name_is_the_utc_start_moment() {
        let name = session_file_name(UNIX_EPOCH + Duration::from_secs(1_789_448_052));
        assert_eq!(name, "2026-09-15_04-54-12Z.log");
        let before = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(session_file_name(before), "1970-01-01_00-00-00Z.log");
    }

    #[test]
    fn prune_keeps_the_newest_sessions_and_spares_the_live_file() {
        let dir = temp_log_dir("prune");
        let write_at = |name: &str, secs: u64| {
            let p = dir.join(name);
            fs::write(&p, b"x").unwrap();
            let f = fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
                .unwrap();
        };
        for i in 0..15u64 {
            write_at(&format!("2026-09-01_00-00-{i:02}Z.log"), 1000 + i);
        }
        write_at("md-test.log", 1);

        let live = dir.join("2026-09-15_00-00-00Z.log");
        fs::write(&live, b"live").unwrap();
        let f = fs::File::options().write(true).open(&live).unwrap();
        f.set_modified(UNIX_EPOCH).unwrap();

        prune_old_logs(&dir, &live);

        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left.len(),
            KEEP_SESSIONS + 1,
            "KEEP old files plus the one being written: {left:?}"
        );
        assert!(
            left.contains(&"2026-09-15_00-00-00Z.log".to_owned()),
            "the file being written is exempt by path: {left:?}"
        );
        assert!(
            !left.contains(&"md-test.log".to_owned()),
            "the old single file must be collected"
        );
        assert!(
            !left.contains(&"2026-09-01_00-00-00Z.log".to_owned()),
            "the oldest session must be collected: {left:?}"
        );
        assert!(left.contains(&"2026-09-01_00-00-14Z.log".to_owned()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_leaves_short_directories_alone() {
        let dir = temp_log_dir("prune-short");
        for i in 0..=KEEP_SESSIONS {
            let p = dir.join(format!("2026-09-01_00-00-{i:02}Z.log"));
            fs::write(p, b"x").unwrap();
        }
        let live = dir.join("2026-09-15_00-00-00Z.log");
        fs::write(&live, b"live").unwrap();
        prune_old_logs(&dir, &live);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), KEEP_SESSIONS + 1);
        assert!(live.exists());
        fs::remove_dir_all(&dir).ok();
    }
}
