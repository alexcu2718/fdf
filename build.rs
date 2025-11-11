#![allow(clippy::all)]
#![allow(warnings)]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::thread;

#[cfg(target_os = "linux")]
/// Checking filesystem support for reiserfs
fn get_supported_filesystems() -> Result<Vec<String>, std::io::Error> {
    let file = File::open("/proc/filesystems")?;
    let reader = BufReader::new(file);
    let mut filesystems = Vec::new();

    for line in reader.lines() {
        if let Ok(line) = line {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(fs_name) = parts.last() {
                let fs_name = fs_name.to_lowercase();
                filesystems.push(fs_name);
            }
        }
    }

    Ok(filesystems)
}

#[cfg(not(target_os = "linux"))]
fn get_supported_filesystems() -> Result<Vec<String>, std::io::Error> {
    // On non-Linux systems, there isn't any Reiser support
    Ok(Vec::new())
}

fn main() {
    // Re-run build script if filesystem list changes
    #[cfg(target_os = "linux")]
    println!("cargo:rerun-if-changed=/proc/filesystems");

    //set threadcounts for rayon.
    const MIN_THREADS: usize = 1;
    let num_threads =
        thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    println!("cargo:rustc-env=THREAD_COUNT={num_threads}");

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!("cargo:rustc-env=FDF_PAGE_SIZE={page_size}");

    // Check for reiser and stop building if so
    #[cfg(target_os = "linux")]
    match get_supported_filesystems() {
        Ok(filesystems) => {
            let has_reiser = filesystems.iter().any(|fs| fs.starts_with("reiser"));
            /// Crash on reiser support
            assert!(!has_reiser, "reiser file systems not supported");

            let has_zfs = filesystems.iter().any(|fs| fs.starts_with("zfs"));

            if has_zfs {
                println!("cargo:rustc-env=HAS_ZFS_FS=TRUE");
            }
        }
        Err(e) => {
            println!("cargo:warning=Failed to read /proc/filesystems: {e}");
        }
    }
}
