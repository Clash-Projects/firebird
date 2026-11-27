use std::sync::atomic::{AtomicBool, Ordering};

static APPLIED: AtomicBool = AtomicBool::new(false);
static SUCCEEDED: AtomicBool = AtomicBool::new(false);

/// Freeze glibc's dynamic `mmap` / `trim` thresholds at their defaults (128 KiB).
///
/// Linux + glibc only; no-op on other platforms or non-glibc libcs.
pub fn apply_allocator_tuning() {
    if APPLIED.swap(true, Ordering::AcqRel) {
        return;
    }

    #[cfg(target_os = "linux")]
    {
        const THRESHOLD: libc::c_int = 128 * 1024;
        const M_TRIM_THRESHOLD: libc::c_int = -1;
        const M_MMAP_THRESHOLD: libc::c_int = -3;

        // Look up dynamically so musl / non-glibc targets never fail to link.
        unsafe {
            let sym = libc::dlsym(libc::RTLD_DEFAULT, b"mallopt\0".as_ptr().cast());
            if !sym.is_null() {
                let mallopt: unsafe extern "C" fn(libc::c_int, libc::c_int) -> libc::c_int =
                    std::mem::transmute(sym);
                let ok = mallopt(M_TRIM_THRESHOLD, THRESHOLD) != 0
                    && mallopt(M_MMAP_THRESHOLD, THRESHOLD) != 0;
                SUCCEEDED.store(ok, Ordering::Release);
            }
        }
    }
}

/// Whether [`apply_allocator_tuning`] actually reached a libc `mallopt`
/// (Linux + glibc only).
pub fn allocator_tuning_applied() -> bool {
    SUCCEEDED.load(Ordering::Acquire)
}

/// Proactively release idle/uncommitted native CRT, libmpv buffer, and Direct3D
/// pages back to the operating system.
///
/// On Windows, calls `SetProcessWorkingSetSize(GetCurrentProcess(), -1, -1)`
/// to flush idle pages back to the OS without terminating any handles.
/// On Linux, invokes `malloc_trim(0)` via `dlsym` if available.
pub fn trim_process_memory() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        unsafe {
            let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }

    #[cfg(target_os = "linux")]
    {
        unsafe {
            let sym = libc::dlsym(libc::RTLD_DEFAULT, b"malloc_trim\0".as_ptr().cast());
            if !sym.is_null() {
                let malloc_trim: unsafe extern "C" fn(libc::size_t) -> libc::c_int =
                    std::mem::transmute(sym);
                let _ = malloc_trim(0);
            }
        }
    }
}
