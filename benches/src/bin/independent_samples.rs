use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Row {
    peak_ws: f64,
    peak_heap: f64,
    steady_heap: f64,
    step_heap: f64,
    media_total: f64,
    step_media: f64,
    peak_media: f64,
    display_bytes: f64,
    warm_bytes: f64,
    dropped: f64,
    display: f64,
    mermaid: f64,
    thaws: f64,
    cools: f64,
    cpu_ms: f64,
    cpu_scroll_ms: f64,
    step_p95: f64,
    step_p99: f64,
}

impl Row {
    fn parse(text: &str) -> Option<Self> {
        let line = text.lines().find(|line| line.contains("] summary "))?;
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            return None;
        }
        let jobs = parts[3]
            .split_once("jobs=")
            .map(|(_, tail)| tail)
            .unwrap_or_default();
        let media_total = mib(parts[3], "total=")?;
        let steady_heap = mib(parts[2], "rust-heap=")?;
        let cost = text
            .lines()
            .find(|line| line.contains("] cost "))
            .unwrap_or_default();
        Some(Self {
            peak_ws: mib(parts[1], " ws=")?,
            peak_heap: mib(parts[1], "rust-heap=")?,
            steady_heap,
            step_heap: step_median(text, "rust-heap=").unwrap_or(steady_heap),
            media_total,
            step_media: step_median(text, "total=").unwrap_or(media_total),
            peak_media: step_max(text, "total=").unwrap_or(media_total),
            display_bytes: step_median(text, "display=").unwrap_or(0.0),
            warm_bytes: step_median(text, "warm=").unwrap_or(0.0),
            dropped: count(parts[3], "warm-dropped=").unwrap_or(0.0),
            display: count(jobs, "display")?,
            mermaid: count(jobs, "mermaid")?,
            thaws: count(jobs, "thaw")?,
            cools: count(jobs, "cool")?,
            cpu_ms: mib(cost, "total=").unwrap_or(0.0),
            cpu_scroll_ms: mib(cost, "scroll=").unwrap_or(0.0),
            step_p95: mib(cost, "p95=").unwrap_or(0.0),
            step_p99: mib(cost, "p99=").unwrap_or(0.0),
        })
    }
}

fn mib(segment: &str, key: &str) -> Option<f64> {
    let start = segment.find(key)? + key.len();
    let rest = &segment[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    let value: f64 = rest[..end].parse().ok()?;
    let unit = &rest[end..];
    Some(if unit.starts_with("KiB") {
        value / 1024.0
    } else if unit.starts_with('B') {
        value / (1024.0 * 1024.0)
    } else {
        value
    })
}

fn count(segment: &str, key: &str) -> Option<f64> {
    let start = segment.find(key)? + key.len();
    let rest = &segment[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn step_median(text: &str, key: &str) -> Option<f64> {
    let mut values = step_values(text, key);
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).expect("finite"));
    let len = values.len();
    Some(if len % 2 == 1 {
        values[len / 2]
    } else {
        (values[len / 2 - 1] + values[len / 2]) / 2.0
    })
}

fn step_max(text: &str, key: &str) -> Option<f64> {
    step_values(text, key)
        .into_iter()
        .reduce(|left, right| if right > left { right } else { left })
}

fn step_values(text: &str, key: &str) -> Vec<f64> {
    text.lines()
        .filter(|line| line.contains("] step="))
        .filter_map(|line| mib(line, key))
        .collect()
}

fn bench_binary() -> PathBuf {
    if let Ok(path) = std::env::var("MD_BENCH_BIN") {
        return PathBuf::from(path);
    }
    let exe = std::env::current_exe().expect("current executable");
    let deps = exe.parent().expect("executable directory").join("deps");
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(&deps).expect("benchmark deps directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !name.starts_with("media_scroll-") {
            continue;
        }
        let suffix = std::env::consts::EXE_SUFFIX;
        if suffix.is_empty() {
            if name.contains('.') {
                continue;
            }
        } else if !name.ends_with(suffix) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(UNIX_EPOCH);
        if newest.as_ref().is_none_or(|(seen, _)| modified > *seen) {
            newest = Some((modified, path));
        }
    }
    newest
        .map(|(_, path)| path)
        .expect("no media_scroll benchmark binary: build it with `cargo bench --bench media_scroll -- --test`")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let filter = args.next().unwrap_or_default();
    let runs: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let bench = bench_binary();

    println!("bench   {}", bench.display());
    println!("filter  {filter:?}");
    println!("runs    {runs}");
    println!();
    println!(
        "{:>4} {:>9} {:>8} {:>8} {:>8} {:>6} {:>6} {:>6} {:>6} {:>5} {:>8} {:>7} {:>7}",
        "run",
        "media",
        "display",
        "warm",
        "peak",
        "disp",
        "merm",
        "thaw",
        "cool",
        "drop",
        "cpu-ms",
        "p95-ms",
        "p99-ms"
    );

    let mut rows = Vec::with_capacity(runs);
    for run in 1..=runs {
        let output = Command::new(&bench)
            .arg(&filter)
            .arg("--test")
            .output()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", bench.display()));
        if !output.status.success() {
            eprintln!(
                "run {run} exited with {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            std::process::exit(1);
        }
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let Some(row) = Row::parse(&text) else {
            eprintln!("run {run} printed no summary line\n{text}");
            std::process::exit(1);
        };
        println!(
            "{:>4} {:>9.2} {:>8.2} {:>8.2} {:>8.2} {:>6.0} {:>6.0} {:>6.0} {:>6.0} {:>5.0} {:>8.0} {:>7.3} {:>7.3}",
            run,
            row.step_media,
            row.display_bytes,
            row.warm_bytes,
            row.peak_media,
            row.display,
            row.mermaid,
            row.thaws,
            row.cools,
            row.dropped,
            row.cpu_ms,
            row.step_p95,
            row.step_p99
        );
        rows.push(row);
    }

    println!();
    println!("across {} independent processes, MiB", rows.len());
    spread(&rows, "peak-ws", |row| row.peak_ws);
    spread(&rows, "peak-heap", |row| row.peak_heap);
    spread(&rows, "step-heap", |row| row.step_heap);
    spread(&rows, "steady-heap", |row| row.steady_heap);
    spread(&rows, "step-media", |row| row.step_media);
    spread(&rows, "display", |row| row.display_bytes);
    spread(&rows, "warm", |row| row.warm_bytes);
    spread(&rows, "peak-media", |row| row.peak_media);
    spread(&rows, "media-total", |row| row.media_total);
    spread(&rows, "display-jobs", |row| row.display);
    spread(&rows, "mermaid-jobs", |row| row.mermaid);
    spread(&rows, "thaw-jobs", |row| row.thaws);
    spread(&rows, "cool-jobs", |row| row.cools);
    spread(&rows, "dropped", |row| row.dropped);
    println!();
    println!("across {} independent processes, milliseconds", rows.len());
    spread(&rows, "cpu-total", |row| row.cpu_ms);
    spread(&rows, "cpu-scroll", |row| row.cpu_scroll_ms);
    spread(&rows, "step-p95", |row| row.step_p95);
    spread(&rows, "step-p99", |row| row.step_p99);
}

fn spread(rows: &[Row], name: &str, pick: fn(&Row) -> f64) {
    let mut values: Vec<f64> = rows.iter().map(pick).collect();
    values.sort_by(|left, right| left.partial_cmp(right).expect("finite"));
    let len = values.len();
    let median = if len % 2 == 1 {
        values[len / 2]
    } else {
        (values[len / 2 - 1] + values[len / 2]) / 2.0
    };
    println!(
        "{name:<12} median {median:>8.2}  min {:>8.2}  max {:>8.2}  spread {:>6.2}",
        values[0],
        values[len - 1],
        values[len - 1] - values[0]
    );
}
