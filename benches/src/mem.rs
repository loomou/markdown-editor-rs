use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub working_set: usize,
    pub peak_working_set: usize,
    pub pagefile: usize,
    pub peak_pagefile: usize,
}

impl fmt::Display for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "working-set={} peak-working-set={} pagefile={} peak-pagefile={}",
            human_bytes(self.working_set),
            human_bytes(self.peak_working_set),
            human_bytes(self.pagefile),
            human_bytes(self.peak_pagefile),
        )
    }
}

pub fn human_bytes(bytes: usize) -> String {
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

#[cfg(not(windows))]
pub fn sample() -> Option<Memory> {
    None
}
