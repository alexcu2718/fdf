# fdf – High-Performance POSIX File Finder

**fdf** is an experimental, high-performance alternative to [`fd`](https://github.com/sharkdp/fd) and `find`, optimised for **regex** and **glob** matching with colourised output.  
Originally a learning project in **advanced Rust**, **C**, and **assembly**, it has evolved into a competitive, benchmarked tool for fast filesystem search.

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](<https://github.com/alexcu2718/fdf/actions>)

Easily installed via:   (FULL INSTRUCTIONS FOUND TOWARDS BOTTOM OF PAGE)

```bash
cargo install --git https://github.com/alexcu2718/fdf
```

## Important Notes

Contributions will be considered once features are stabilised and improved. This remains a learning/hobby project requiring significant development.

(Although if someone really wants to contribute, go nuts!)

The implemented subset performs well, surpassing fd in equivalent feature sets, though fd offers a broader range. The project focuses on exploring hardware-specific code optimisation rather than replicating fd's full functionality. Ultimately I wanted a really fast regex/glob tool for myself and learning how to program at a low level.

## Platform Support Status (64 bit only, 32 bit not planned)

### Automatically Tested via GitHub Actions CI/CD

 **Fully Supported & CI Tested**: Linux (x86_64, aarch64, s390x, RISC-V64), macOS (Intel & Apple Silicon), FreeBSD(x86_64)

 **Compiles but Limited Testing**: OpenBSD, NetBSD,  DragonflyBSD, Android  (Not familiar with github actions for these ones)

 **Not Supported**: Windows (fundamental rewrite required due to architectural differences, will be done when I read through the API properly!)

## How to test

```bash
git clone https://github.com/alexcu2718/fdf /tmp/fdf_test  && cd /tmp/fdf_test/fd_benchmarks &&   ./run_all_tests_USE_ME.sh
```

This executes a comprehensive suite of internal library, CLI tests, and benchmarks.

## Cool bits(full benchmarks can be seen in speed_benchmarks.txt)

Testing on my local filesystem (to show on non-toy example)

```bash
| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf ''  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 | #search for symlinks
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |


| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 |
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |



```

 **Full Benchmarks:** [Found here](https://github.com/alexcu2718/fdf/blob/main/speed_benchmarks.txt)

## Extra bits

-cstr! :a macro  use a byte slice as a pointer (automatically initialise memory(no heap use), then add a **null terminator** for FFI use)

-find_char_in_word: Find the first occurrence of a byte in a 64-bit word (Using SWAR(SIMD within a register)), a const fn

-A black magic macro that can colour filepaths based on a compile time perfect hashmap
it's defined in another github repo of mine at <https://github.com/alexcu2718/compile_time_ls_colours>

Then this function, really nice way to avoid branch misses during dirent parsing (a really hot loop)

```rust

//The code is explained better in the true function definition (this is crate agnostic)
//This is the little-endian implementation, see crate for modified version for big-endian
// Only used on Linux systems, OpenBSD/macos systems store the name length trivially (no clue on Windows because reading the API is AWFUL)
//(SIMD within a register, so no architecture dependence)
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    //the only true unsafe action here is dereferencing the pointer, that MUST be checked beforehand
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; 
    //reclen is always multiple of 8 so alignment is guaranteed (unaligned reads are expensive!)
    //endianness fix omitted for brevity. check source
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch
    let candidate_pos = last_word | mask;//^
    let byte_pos = 7 -  find_zero_byte_u64(candidate_pos) ; // no branch SWAR
    reclen - DIRENT_HEADER_START - byte_pos
}


```

## Why?

I started this project because I found find slow and wanted to learn how to interface directly with the kernel.
What began as a random experiment turned out to be a genuinely useful tool - one I'll probably use for the rest of my life to find files efficiently.

At the core, this is about learning. When I began, I didn't even know C, so there are some rough ABI edges. But along the way, I've picked up low-level skills and this project has been really useful for that!

### Performance Motivation

Even though fdf is already faster than fd in some cases, I'm experimenting with filtering before allocation.
Rust's std::fs has some inefficiencies, notably more heap allocation than I'd like. Rewriting certain parts using libc was the ideal way to bypass that and learn in the process.

Currently, filtering-before-allocation is partially implemented in the crate but not yet exposed via the CLI. If the results prove consistently performant, I'll integrate it into the public tool.

### Development Philosophy

* Feature stability before breakage - I won't push breaking changes until I'm confident they're worth it.

* Open to contributions - Once the codebase stabilises, I welcome others to add features if they're extremely inclined anyway!

* Pragmatic focus - Some areas, like datetime filtering, are on hold simply because I rarely use them. They may be added in the future, especially if someone else is motivated to implement them.

In short, this project is a personal exploration into performance, low-level programming, and building practical tools - with the side benefit that it's actually good at what it does.

## NECESSARY DISCLAIMERS

I've directly taken code from <https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574> and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for here <https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161>
I've found a much more rigorous way of doing some bit tricks via this

I enjoy relying on  validated work like stdlib to ideally 'covalidate' my work, aka less leaps of logic required for others to validate.

## Future Plans

### Modularization

While avoiding excessive fragmentation, I plan to extract reusable components (like platform-specific FFI utilities) into separate crates. This will improve maintainability without sacrificing the project's cohesive design.

### Feature Enhancements

*DateTime Filtering**: Fast, attribute-based file filtering by time (high priority despite personal infrequent use).
*Extended File Types**: Support for searching device drivers, and other special files.
*POSIX Compliance**: Broader support for Illumos/Solaris and other POSIX systems (currently challenging due to QEMU complexities(and laziness)).

### Platform Expansion

**Windows Support**: Acknowledged as a significant undertaking requiring architectural changes, but valuable for both usability and learning Windows internals.

### Tooling Exploration

**Compile-Time Techniques**: Further development of `compile_time_ls_colours` to explore advanced metaprogramming, mostly because it's interesting (doubt I can add much to it now but I think I could use similar techniques elsewhere!)

### Core Philosophy

The CLI will remain **simple** (avoiding overwhelming help menus(looking at you, ripgrep!)) and **efficient** (prioritising performance in both design and implementation).

## Installation

```bash
# Clone & build
git clone https://github.com/alexcu2718/fdf.git
cd fdf
cargo build --release

# Optional system install
cargo install --git https://github.com/alexcu2718/fdf


Usage
Arguments
PATTERN: Regular expression pattern to search for
PATH: Directory to search (defaults to current directory )
Basic Examples


# Find all JPG files in the home directory (excluding hidden files)
fdf . ~ -E jpg

# Find all  Python files in /usr/local (including hidden files)
fdf . /usr/local -E py -H

## Options (T)

Usage: fdf [OPTIONS] [PATTERN] [PATH]

Arguments:
  [PATTERN]  Pattern to search for
  [PATH]     Path to search (defaults to current working directory )


Options: 

  -s, --case-sensitive         Enable case-sensitive matching, defaults to false

  -j, --threads <THREAD_NUM>   Number of threads to use, defaults to available threads
                                [default: 12]
  -a, --absolute-path          Show absolute paths of results, defaults to false

  -I, --include-dirs           Include directories, defaults to off

  -L, --follow                 Include symlinks in traversal,defaults to false

  -g, --glob                   Use a glob pattern,defaults to off

  -n, --max-results <TOP_N>    Retrieves the first eg 10 results, '.cache' / -n 10

  -d, --depth <DEPTH>          Retrieves only traverse to x depth

      --generate <GENERATE>    Generate shell completions
                                [possible values: bash, elvish, fish, powershell, zsh]
  -t, --type <TYPE_OF>...      Select type of files (can use multiple times).
                                Available options are:
                               d: Directory
                               u: Unknown
                               l: Symlink
                               f: Regular File
                               p: Pipe
                               c: Char Device
                               b: Block Device
                               s: Socket
                               e: Empty
                               x: Executable
  -p, --full-path              Use a full path for regex matching, default to false

  -F, --fixed-strings          Use a fixed string not a regex, defaults to false

  -h, --help                   Print help
  -V, --version                Print version

```

### Potential Future Enhancements

**1. Custom Arena Allocator**  
-- Investigate implementing from scratch  
-- Reference implementation: [Microsoft's Edit Arena](https://github.com/microsoft/edit/tree/main/src/arena)  

**2. io_uring System Call Batching**  
-- Explore batched open/read operations  
-- Significant challenges:  
-- Current lack of `getdents` support in io_uring  
-- Necessitates async runtime integration (potential Tokio dependency)  
-- Conflicts with minimal-dependency philosophy  
-- Linux only, that isn't too appealing for such a difficult addition.

**3. Compile-time LS_COLORS Enhancement**  
-- Develop dependency-free version:  
  [No-PHF Implementation](https://github.com/alexcu2718/compile_time_ls_colours/tree/no_phf_build)  
-- Address current limitations:  
-- Manual byte manipulation requirements  
-- Runtime static initialisation  

**4. Native Threading Implementation**  
-- Replace Rayon dependency  
-- Develop custom work-distribution algorithm  
-- Current status: Experimental approaches underway  

**5. Allocation-Optimised Iterator Adaptor**  
-- Design filter mechanism avoiding:  
-- Unnecessary directory allocations  
-- Non-essential memory operations  

**6. Syscall Efficiency Research**  
-- Investigate hard limits of system calls  
-- Experimental Zig-based io_uring integration:  
-- Currently outside comfort zone  
-- Considered valuable learning opportunity  
