#![allow(clippy::all)]
#![allow(warnings)]

use std::io::Write;
use std::thread;

fn main() {
    //set threadcounts for rayon.
    const MIN_THREADS: usize = 1;
    let num_threads =
        thread::available_parallelism().map_or(MIN_THREADS, core::num::NonZeroUsize::get);

    if num_threads == MIN_THREADS {
        println!("cargo:rustc-env=CPU_COUNT={MIN_THREADS}");
    } else {
        println!("cargo:rustc-env=CPU_COUNT={num_threads}");
    }
}
