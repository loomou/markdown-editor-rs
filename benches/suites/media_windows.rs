use gpui::BenchAppContext;
use md_bench::gpui_support::{FrameCase, media_cases, mount_shell};
use md_bench::mem;
use md_bench::scroll_probe::{run_windows_pass, summarize_windows};

const STEPS: usize = 200;

/// The two cases whose caches are already short of room on their own. A window
/// that can hold everything shows nothing about sharing: the interesting case
/// is two windows competing for one budget.
fn window_cases() -> Vec<FrameCase> {
    media_cases()
        .into_iter()
        .filter(|case| {
            matches!(
                case.label.as_str(),
                "media-images-12-tight" | "media-mixed-12-tight"
            )
        })
        .collect()
}

#[gpui::bench(inputs = window_cases(), input_name = "case", group = "media/windows", sample_size = 10)]
fn media_windows(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    if std::env::var("MD_BENCH_COOLING").as_deref() == Ok("0") {
        md_content::media::set_cooling(false);
    }
    if let Some(millis) = std::env::var("MD_BENCH_COOL_GRACE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        md_content::media::set_cool_grace(std::time::Duration::from_millis(millis));
    }
    mem::reset_peak();
    cx.update(md_content::media::MediaCoordinator::reset_peak);
    let first = mount_shell(case, cx);
    let second = mount_shell(case, cx);
    let pass = run_windows_pass(&[first, second], cx, &case.label, STEPS);
    summarize_windows(&case.label, &pass);
}

gpui::bench_group!(benches, media_windows);
gpui::bench_main!(benches);
