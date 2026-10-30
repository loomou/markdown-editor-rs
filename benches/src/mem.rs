use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub working_set: usize,
    pub peak_working_set: usize,
    pub pagefile: usize,
    pub peak_pagefile: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Heap {
    pub live: usize,
    pub peak: usize,
}

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

pub struct CountingAllocator;

fn record(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    let mut peak = PEAK.load(Ordering::Relaxed);
    while live > peak {
        match PEAK.compare_exchange_weak(peak, live, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(current) => peak = current,
        }
    }
}

fn forget(size: usize) {
    LIVE.fetch_sub(size, Ordering::Relaxed);
}

// SAFETY: every method forwards to the System allocator with the layout it was
// given, and only adjusts the counters after the real allocator succeeded, so
// the accounting never disagrees with the pointer the caller receives.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller of an allocator method guarantees a valid layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller of an allocator method guarantees a valid layout.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer came from this allocator with the same layout.
        unsafe { System.dealloc(ptr, layout) };
        forget(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer came from this allocator with the same layout.
        let out = unsafe { System.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            forget(layout.size());
            record(new_size);
        }
        out
    }
}

pub fn heap() -> Heap {
    Heap {
        live: LIVE.load(Ordering::Relaxed),
        peak: PEAK.load(Ordering::Relaxed),
    }
}

pub fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

pub fn heap_text() -> String {
    let h = heap();
    format!(
        "rust-heap={} peak-rust-heap={}",
        format_bytes(h.live),
        format_bytes(h.peak)
    )
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "working-set={} peak-working-set={} pagefile={} peak-pagefile={}",
            format_bytes(self.working_set),
            format_bytes(self.peak_working_set),
            format_bytes(self.pagefile),
            format_bytes(self.peak_pagefile),
        )
    }
}

pub fn format_bytes(bytes: usize) -> String {
    const KIB: usize = 1 << 10;
    const MIB: usize = 1 << 20;
    const GIB: usize = 1 << 30;
    if bytes >= GIB {
        format!("{}.{:02}GiB", bytes / GIB, (bytes % GIB) * 100 / GIB)
    } else if bytes >= MIB {
        format!("{}.{:02}MiB", bytes / MIB, (bytes % MIB) * 100 / MIB)
    } else if bytes >= KIB {
        format!("{}.{:02}KiB", bytes / KIB, (bytes % KIB) * 100 / KIB)
    } else {
        format!("{bytes}B")
    }
}

pub fn format_mib(bytes: usize) -> String {
    const MIB: usize = 1 << 20;
    format!("{}.{:02}MiB", bytes / MIB, (bytes % MIB) * 100 / MIB)
}

pub fn sample_text() -> String {
    match sample() {
        Some(memory) => memory.to_string(),
        None => "n/a".to_owned(),
    }
}

#[cfg(windows)]
pub fn sample() -> Option<Memory> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: GetCurrentProcess returns a pseudo-handle that is always valid for the calling
    // process and must never be closed, and counters stays alive across the call with its cb
    // field reporting the exact size of the struct that GetProcessMemoryInfo writes into.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        return None;
    }
    Some(Memory {
        working_set: counters.WorkingSetSize,
        peak_working_set: counters.PeakWorkingSetSize,
        pagefile: counters.PagefileUsage,
        peak_pagefile: counters.PeakPagefileUsage,
    })
}

#[cfg(windows)]
pub fn private_working_set() -> Option<usize> {
    use core::ffi::c_void;
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, VirtualQueryEx,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        PSAPI_WORKING_SET_EX_INFORMATION, QueryWorkingSetEx,
    };
    use windows_sys::Win32::System::SystemInformation::{GetNativeSystemInfo, SYSTEM_INFO};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    const BATCH: usize = 4096;
    const VALID: usize = 1;
    const SHARED: usize = 1 << 15;

    // SAFETY: GetCurrentProcess returns a pseudo-handle that is always valid for the
    // calling process and must never be closed.
    let handle = unsafe { GetCurrentProcess() };
    let mut system = SYSTEM_INFO::default();
    // SAFETY: GetNativeSystemInfo writes only into the SYSTEM_INFO we own.
    unsafe { GetNativeSystemInfo(&mut system) };
    let page = system.dwPageSize as usize;
    if page == 0 {
        return None;
    }

    let mut batch = vec![PSAPI_WORKING_SET_EX_INFORMATION::default(); BATCH];
    let mut private = 0usize;
    let mut cursor = 0usize;
    loop {
        let mut region = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: handle is the current-process pseudo-handle, cursor is only an address
        // hint, and region is owned by us with dwlength reporting its exact size.
        let written = unsafe {
            VirtualQueryEx(
                handle,
                cursor as *const c_void,
                &mut region,
                core::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if written == 0 {
            break;
        }
        let base = region.BaseAddress as usize;
        let size = region.RegionSize;
        if region.State == MEM_COMMIT {
            let end = base.saturating_add(size);
            let mut at = base;
            while at < end {
                let take = ((end - at) / page).min(BATCH);
                if take == 0 {
                    break;
                }
                for (offset, entry) in batch.iter_mut().take(take).enumerate() {
                    entry.VirtualAddress = (at + offset * page) as *mut c_void;
                }
                let bytes =
                    (take * core::mem::size_of::<PSAPI_WORKING_SET_EX_INFORMATION>()) as u32;
                // SAFETY: handle is the current-process pseudo-handle, the buffer holds
                // `take` initialised entries, and bytes is exactly their size.
                let ok = unsafe { QueryWorkingSetEx(handle, batch.as_mut_ptr().cast(), bytes) };
                if ok != 0 {
                    for entry in batch.iter().take(take) {
                        // SAFETY: the working set block is a plain bitfield, so reading
                        // its raw flags is always valid.
                        let flags = unsafe { entry.VirtualAttributes.Flags };
                        if flags & VALID != 0 && flags & SHARED == 0 {
                            private += page;
                        }
                    }
                }
                at += take * page;
            }
        }
        let next = base.saturating_add(size);
        if next <= cursor {
            break;
        }
        cursor = next;
    }
    Some(private)
}

#[cfg(not(windows))]
pub fn sample() -> Option<Memory> {
    None
}

#[cfg(not(windows))]
pub fn private_working_set() -> Option<usize> {
    None
}
