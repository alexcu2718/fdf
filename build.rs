#![allow(clippy::all)]
#![allow(warnings)]

use std::io::Write;
use std::thread;

#[cfg(target_os = "macos")]
pub unsafe fn getdirentries64(
    fd: libc::c_int,
    buffer_ptr: *mut i8,
    nbytes: libc::size_t,
    basep: *mut libc::off_t,
) -> i32 {
    const SYS_GETDIRENTRIES64: libc::c_int = 344; // Reverse engineered syscall number
    //https://phrack.org/issues/66/16
    unsafe { libc::syscall(SYS_GETDIRENTRIES64, fd, buffer_ptr, nbytes, basep) }
}

#[cfg(target_os = "macos")]
// Because we're rawdogging an undocumented syscall, we need to check if it doesnt change
// So this SHOULD always return >=0 from the syscall
fn test_macos_syscall() {
    use std::env::temp_dir;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::AsRawFd;
    let tmp = temp_dir();
    let test_dir = CString::new(tmp.as_os_str().as_bytes()).expect("CString conversion failed");

    unsafe {
        let dir_fd = libc::open(test_dir.as_ptr(), libc::O_RDONLY);
        if dir_fd < 0 {
            panic!("Failed to open directory for testing");
        }

        let mut buffer: [i8; 1024] = [0; 1024];
        let mut basep: libc::off_t = 0;

        let result = getdirentries64(dir_fd, buffer.as_mut_ptr(), buffer.len(), &mut basep);

        libc::close(dir_fd);

        // Note: A result of 0 means no more entries, which is valid
        // Negative values indicate errors
        if result < 0 {
            panic!(
                "getdirentries64 syscall test failed with result: {result}\n This indicates the syscall number has changed!",
            );
        }
    }
}

fn main() {
    //set threadcounts for rayon.
    const MIN_THREADS: usize = 1;
    let num_threads =
        thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    println!("cargo:rustc-env=THREAD_COUNT={num_threads}");

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!("cargo:rustc-env=FDF_PAGE_SIZE={page_size}");

    let max_filename_len = unsafe { libc::pathconf(c"/".as_ptr(), libc::_PC_NAME_MAX) };
    println!("cargo:rustc-env=NAME_MAX={max_filename_len}");
    assert!(
        max_filename_len >= 255,
        "NAME_MAX is not appropriately set!"
    );

    #[cfg(target_os = "macos")]
    test_macos_syscall();
}
