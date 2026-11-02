#[cfg(not(windows))]
fn main() {
    eprintln!("app_measure drives a real window and only builds on windows");
}

#[cfg(windows)]
fn main() {
    windows_impl::main();
}

#[cfg(windows)]
mod windows_impl {
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, ClientToScreen, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
        PW_RENDERFULLCONTENT, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
        WM_CLOSE, WM_MOUSEWHEEL,
    };

    const WHEEL_DELTA: i32 = 120;
    const NOTCH_SLEEP: Duration = Duration::from_millis(16);
    const SETTLE: Duration = Duration::from_millis(1500);
    const RESIZE_SETTLE: Duration = Duration::from_millis(600);
    const SHOT_DELAY: Duration = Duration::from_millis(1500);
    const SAMPLE_EVERY: usize = 4;
    const WINDOW_WAIT: Duration = Duration::from_secs(30);
    const CLOSE_WAIT: Duration = Duration::from_secs(10);

    struct Args {
        exe: PathBuf,
        doc: PathBuf,
        label: String,
        passes: usize,
        notches: usize,
        width: i32,
        height: i32,
        keep_open: bool,
        shot: Option<PathBuf>,
    }

    fn parse_args() -> Args {
        let mut exe = PathBuf::from("target/debug/markdown-editor-rs.exe");
        let mut doc = None;
        let mut label = None;
        let mut passes = 4;
        let mut notches = 40;
        let mut width = 900;
        let mut height = 700;
        let mut keep_open = false;
        let mut shot = None;
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut index = 0;
        while index < raw.len() {
            let key = raw[index].as_str();
            let mut next = || {
                index += 1;
                raw.get(index).cloned().unwrap_or_default()
            };
            match key {
                "--exe" => exe = PathBuf::from(next()),
                "--doc" => doc = Some(PathBuf::from(next())),
                "--label" => label = Some(next()),
                "--passes" => passes = next().parse().unwrap_or(passes),
                "--notches" => notches = next().parse().unwrap_or(notches),
                "--width" => width = next().parse().unwrap_or(width),
                "--height" => height = next().parse().unwrap_or(height),
                "--shot" => shot = Some(PathBuf::from(next())),
                "--keep-open" => keep_open = true,
                other => {
                    eprintln!("unknown argument {other}");
                    std::process::exit(2);
                }
            }
            index += 1;
        }
        let doc = doc.unwrap_or_else(|| {
            eprintln!("--doc is required");
            std::process::exit(2);
        });
        let label = label.unwrap_or_else(|| {
            doc.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("document")
                .to_owned()
        });
        Args {
            exe,
            doc,
            label,
            passes,
            notches,
            width,
            height,
            keep_open,
            shot,
        }
    }

    static TARGET_PID: AtomicU32 = AtomicU32::new(0);
    static FOUND: AtomicIsize = AtomicIsize::new(0);

    unsafe extern "system" fn find_window(hwnd: HWND, _lparam: LPARAM) -> i32 {
        let mut pid = 0u32;
        // SAFETY: hwnd comes from the enumeration, pid is a valid out-parameter for the
        // duration of the call.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        // SAFETY: hwnd comes from the enumeration and is only queried, never used after.
        if pid == TARGET_PID.load(Ordering::Relaxed) && unsafe { IsWindowVisible(hwnd) } != 0 {
            FOUND.store(hwnd as isize, Ordering::Relaxed);
            return 0;
        }
        1
    }

    fn wait_for_window() -> Option<HWND> {
        let started = Instant::now();
        while started.elapsed() < WINDOW_WAIT {
            FOUND.store(0, Ordering::Relaxed);
            // SAFETY: the callback only reads the enumeration's own handle and writes to
            // statics it owns.
            unsafe { EnumWindows(Some(find_window), 0) };
            let found = FOUND.load(Ordering::Relaxed);
            if found != 0 {
                return Some(found as HWND);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    struct Counters {
        commit: usize,
        working_set: usize,
        peak_working_set: usize,
        pagefile: usize,
    }

    fn read_counters(handle: *mut core::ffi::c_void) -> Option<Counters> {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: handle came from OpenProcess with query rights and stays open across the
        // call; counters reports its own size through cb.
        let ok = unsafe {
            GetProcessMemoryInfo(
                handle,
                (&raw mut counters).cast(),
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        Some(Counters {
            commit: counters.PrivateUsage,
            working_set: counters.WorkingSetSize,
            peak_working_set: counters.PeakWorkingSetSize,
            pagefile: counters.PagefileUsage,
        })
    }

    fn mib(bytes: usize) -> String {
        format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
    }

    fn capture(hwnd: HWND, path: &std::path::Path) -> Option<(u32, u32, i32)> {
        // SAFETY: hwnd belongs to the child and stays alive across the call; every GDI
        // handle created here is released before returning.
        unsafe {
            let mut window = RECT::default();
            if GetWindowRect(hwnd, &mut window) == 0 {
                return None;
            }
            let mut client = RECT::default();
            if GetClientRect(hwnd, &mut client) == 0 {
                return None;
            }
            let mut origin = POINT { x: 0, y: 0 };
            if ClientToScreen(hwnd, &mut origin) == 0 {
                return None;
            }
            let width = window.right - window.left;
            let height = window.bottom - window.top;
            if width <= 0 || height <= 0 {
                return None;
            }

            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                memory,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(memory);
                ReleaseDC(std::ptr::null_mut(), screen);
                return None;
            }
            let previous = SelectObject(memory, bitmap);
            let printed = PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT);
            SelectObject(memory, previous);

            let stride = width as usize * 4;
            let raw = std::slice::from_raw_parts(bits as *const u8, stride * height as usize);
            let left = (origin.x - window.left).clamp(0, width);
            let top = (origin.y - window.top).clamp(0, height);
            let crop_w = (client.right - client.left).clamp(0, width - left) as u32;
            let crop_h = (client.bottom - client.top).clamp(0, height - top) as u32;
            let mut rgba = Vec::with_capacity(crop_w as usize * crop_h as usize * 4);
            for y in 0..crop_h as usize {
                let row = (top as usize + y) * stride + left as usize * 4;
                for x in 0..crop_w as usize {
                    let px = &raw[row + x * 4..row + x * 4 + 4];
                    rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                }
            }

            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);

            image::RgbaImage::from_raw(crop_w, crop_h, rgba)?
                .save(path)
                .ok()?;
            Some((crop_w, crop_h, printed))
        }
    }

    #[derive(Default)]
    struct Collected {
        frames: Vec<(f64, f64)>,
        atlas: Vec<u64>,
        media: Vec<String>,
    }

    fn collect(child: &mut Child) -> Arc<Mutex<Collected>> {
        let sink: Arc<Mutex<Collected>> = Arc::new(Mutex::new(Collected::default()));
        let Some(stderr) = child.stderr.take() else {
            return sink;
        };
        let out = Arc::clone(&sink);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let Some((_, rest)) = line.split_once("md-measure ") else {
                    continue;
                };
                let Ok(mut sink) = out.lock() else { return };
                if let Some(fields) = rest.strip_prefix("frame ") {
                    let mut parts = fields.split(' ');
                    if let (Some(interval), Some(prepaint)) = (parts.next(), parts.next())
                        && let (Ok(interval), Ok(prepaint)) =
                            (interval.parse::<f64>(), prepaint.parse::<f64>())
                    {
                        sink.frames.push((interval, prepaint));
                        if let Some(atlas) = parts.next().and_then(|v| v.parse::<u64>().ok()) {
                            sink.atlas.push(atlas);
                        }
                    }
                } else if let Some(media) = rest.strip_prefix("media ") {
                    sink.media.push(media.to_owned());
                }
            }
        });
        sink
    }

    fn percentile(values: &mut [f64], p: f64) -> f64 {
        if values.is_empty() {
            return f64::NAN;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let index = (((values.len() - 1) as f64) * p).round() as usize;
        values[index]
    }

    pub fn main() {
        // SAFETY: the call only sets this process's DPI awareness before any window work.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let args = parse_args();
        if !args.exe.exists() {
            eprintln!(
                "{} does not exist; build it first with `cargo build -p md-editor --bin markdown-editor-rs`",
                args.exe.display()
            );
            std::process::exit(2);
        }

        let mut child = Command::new(&args.exe)
            .arg(&args.doc)
            .env("MARKDOWN_EDITOR_RS_MEASURE", "1")
            .stderr(Stdio::piped())
            .spawn()
            .expect("the editor must start");
        let collected = collect(&mut child);
        let pid = child.id();
        TARGET_PID.store(pid, Ordering::Relaxed);

        let Some(hwnd) = wait_for_window() else {
            eprintln!(
                "[app-measure {}] no window appeared within 30 s",
                args.label
            );
            let _ = child.kill();
            std::process::exit(1);
        };
        std::thread::sleep(SETTLE);

        if args.width > 0 && args.height > 0 {
            // SAFETY: hwnd is a live window owned by the child process and the call only
            // reads its DPI.
            let scale = unsafe { GetDpiForWindow(hwnd) } as f64 / 96.0;
            let width = (f64::from(args.width) * scale).round() as i32;
            let height = (f64::from(args.height) * scale).round() as i32;
            // SAFETY: hwnd is a live window owned by the child process and the call only
            // resizes it.
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    width,
                    height,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            std::thread::sleep(RESIZE_SETTLE);
        }

        if let Some(shot) = args.shot.clone() {
            std::thread::sleep(SHOT_DELAY);
            match capture(hwnd, &shot) {
                Some((width, height, printed)) => {
                    // SAFETY: hwnd is live; the call only reads its DPI.
                    let dpi = unsafe { GetDpiForWindow(hwnd) };
                    eprintln!(
                        "[app-measure {}] shot {} {width}x{height} dpi={dpi} printwindow={printed}",
                        args.label,
                        shot.display()
                    )
                }
                None => eprintln!("[app-measure {}] shot failed", args.label),
            }
            // SAFETY: hwnd is live and the message carries no payload.
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
            let deadline = Instant::now() + CLOSE_WAIT;
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return;
        }

        // SAFETY: pid names the child this process started, and the handle is closed below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            eprintln!("[app-measure {}] OpenProcess failed", args.label);
            let _ = child.kill();
            std::process::exit(1);
        }

        let mut rect = Default::default();
        // SAFETY: hwnd is live and rect is a valid out-parameter.
        unsafe { GetWindowRect(hwnd, &mut rect) };
        let centre_x = (rect.left + rect.right) / 2;
        let centre_y = (rect.top + rect.bottom) / 2;
        let lparam = ((centre_y as isize) << 16) | ((centre_x as isize) & 0xffff);

        let base = read_counters(handle).expect("the process must report its counters");
        let mut peak_commit = base.commit;
        let mut peak_ws = base.working_set;
        let mut samples = 0usize;
        if let Ok(mut sink) = collected.lock() {
            sink.frames.clear();
            sink.atlas.clear();
        }
        let started = Instant::now();

        for pass in 0..args.passes {
            for notch in 0..args.notches {
                let wparam = ((-WHEEL_DELTA) as usize) << 16;
                // SAFETY: hwnd is live and the message only carries a wheel delta.
                unsafe { PostMessageW(hwnd, WM_MOUSEWHEEL, wparam, lparam) };
                std::thread::sleep(NOTCH_SLEEP);
                if (pass * args.notches + notch).is_multiple_of(SAMPLE_EVERY)
                    && let Some(counters) = read_counters(handle)
                {
                    peak_commit = peak_commit.max(counters.commit);
                    peak_ws = peak_ws.max(counters.working_set);
                    samples += 1;
                }
            }
        }

        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let end = read_counters(handle).expect("the process must still report its counters");
        let peak_commit = peak_commit.max(end.commit);
        let peak_ws = peak_ws.max(end.working_set).max(end.peak_working_set);

        eprintln!(
            "[app-measure {}] window={}x{} notches={} samples={samples} elapsed={elapsed:.0}ms | commit base={} peak={} delta={} | ws base={} peak={} delta={} | pagefile end={} | child={pid}",
            args.label,
            args.width,
            args.height,
            args.passes * args.notches,
            mib(base.commit),
            mib(peak_commit),
            mib(peak_commit.saturating_sub(base.commit)),
            mib(base.working_set),
            mib(peak_ws),
            mib(peak_ws.saturating_sub(base.working_set)),
            mib(end.pagefile),
        );

        let (mut intervals, mut prepaint, first) = match collected.lock() {
            Ok(mut sink) => {
                let first = if sink.frames.is_empty() {
                    f64::NAN
                } else {
                    sink.frames.remove(0).0
                };
                let (intervals, paint) = sink
                    .frames
                    .iter()
                    .map(|&(interval, paint)| (interval, paint))
                    .unzip();
                (intervals, paint, first)
            }
            Err(_) => (Vec::new(), Vec::new(), f64::NAN),
        };
        let over = intervals.iter().filter(|ms| **ms > 33.4).count();
        let frames = intervals.len();
        let atlas = match collected.lock() {
            Ok(sink) => match (sink.atlas.first().copied(), sink.atlas.last().copied()) {
                (Some(first), Some(last)) => Some((first, last)),
                _ => None,
            },
            Err(_) => None,
        };
        eprintln!(
            "[app-measure {}] frames={frames} first={first:.2} interval p50={:.2} p95={:.2} p99={:.2} max={:.2} dropped={over} | prepaint p50={:.2} p95={:.2} p99={:.2} max={:.2}",
            args.label,
            percentile(&mut intervals, 0.5),
            percentile(&mut intervals, 0.95),
            percentile(&mut intervals, 0.99),
            percentile(&mut intervals, 1.0),
            percentile(&mut prepaint, 0.5),
            percentile(&mut prepaint, 0.95),
            percentile(&mut prepaint, 0.99),
            percentile(&mut prepaint, 1.0),
        );
        if let Some((first, last)) = atlas {
            eprintln!(
                "[app-measure {}] atlas {first} -> {last} uploads={}",
                args.label,
                last.saturating_sub(first)
            );
        }

        // SAFETY: the handle came from OpenProcess and is not used afterwards.
        unsafe { CloseHandle(handle) };

        if args.keep_open {
            let _ = child.wait();
        } else {
            // SAFETY: hwnd is live and the message carries no payload.
            unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
            let deadline = Instant::now() + CLOSE_WAIT;
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            let media = match collected.lock() {
                Ok(sink) => sink.media.clone(),
                Err(_) => Vec::new(),
            };
            for line in media {
                eprintln!("[app-measure {}] {line}", args.label);
            }
        }
    }
}
