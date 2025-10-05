#![allow(clippy::all)]
#![allow(warnings)]

use std::io::Write;
use std::thread;

fn main() {
    //set threadcounts for rayon.
    const MIN_THREADS: usize = 1;
    let num_threads =
        thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    println!("cargo:rustc-env=THREAD_COUNT={num_threads}");

    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!("cargo:rustc-env=FDF_PAGE_SIZE={}", page_size);
}
