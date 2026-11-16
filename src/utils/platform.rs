#[cfg(target_os = "linux")]
pub fn thread_count() -> Option<usize> {
    std::fs::read_dir("/proc/self/task").ok().map(|dir| dir.count())
}

#[cfg(windows)]
pub fn thread_count() -> Option<usize> {
    // THREADENTRY32
    #[repr(C)]
    struct ThreadEntry32 {
        dw_size: u32,
        cnt_usage: u32,
        th32_thread_id: u32,
        th32_owner_process_id: u32,
        tp_base_pri: i32,
        tp_delta_pri: i32,
        dw_flags: u32,
    }
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut std::ffi::c_void;
        fn Thread32First(snapshot: *mut std::ffi::c_void, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snapshot: *mut std::ffi::c_void, entry: *mut ThreadEntry32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn GetCurrentProcessId() -> u32;
    }

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        // INVALID_HANDLE_VALUE 是 -1
        if snapshot as isize == -1 {
            return None;
        }
        let me = GetCurrentProcessId();
        let mut entry: ThreadEntry32 = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<ThreadEntry32>() as u32;
        let mut count = 0usize;
        let mut ok = Thread32First(snapshot, &mut entry);
        while ok != 0 {
            if entry.th32_owner_process_id == me {
                count += 1;
            }
            ok = Thread32Next(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        Some(count)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn thread_count() -> Option<usize> {
    None
}