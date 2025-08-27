# fdf – High-Performance POSIX File Finder

**fdf** is an experimental, high-performance alternative to [`fd`](https://github.com/sharkdp/fd) and [`find`](https://www.man7.org/linux/man-pages/man1/find.1.html), optimised for **regex** and **glob** matching with colourised output.  
Originally a learning project in **advanced Rust**, **C**, and a little bit of **assembly**, it has evolved into a competitive, benchmarked tool for fast filesystem search.

--*NOTE, THIS WILL BE RENAMED BEFORE A 1.0, MOSTLY BECAUSE I THOUGHT FD FASTER WAS A FUNNY NAME, SORRY! (awful sense of humour)*

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](<https://github.com/alexcu2718/fdf/actions>)

Easily installed via:   (FULL INSTRUCTIONS FOUND TOWARDS BOTTOM OF PAGE)

```bash
cargo install --git https://github.com/alexcu2718/fdf
```

## Important Notes

Contributions will be considered once features are stabilised and improved. This remains a learning/hobby project requiring *significant* development.

(Although if someone really wants to contribute, go nuts!)

The implemented subset performs exceptionally well, surpassing fd in equivalent feature sets, though fd offers a broader range. The project focuses on exploring hardware-specific code optimisation rather than replicating fd's full functionality. Ultimately I wanted a really fast regex/glob tool for myself and learning how to program at a low level.

## Platform Support Status (64 bit only, 32 bit not planned)

### Automatically Tested via GitHub Actions CI/CD

 **Fully Supported & CI Tested**: Linux (x86_64, aarch64, s390x, RISC-V64,Alpine(MUSL)), macOS (Intel & Apple Silicon), FreeBSD(x86_64),

 **Compiles but Limited Testing**: OpenBSD/NetBSD/DragonflyBSD(tested a few times, only minor fixes would even be needed if broken), Android(works on my phone!), Illumos/Solaris (x86_64)(QEMU tested for verification)

 (Side comment, I am running out of disk space for virtual machines!)

 These platforms don't support rust 2024 yet via github actions, I will add in checks when they do!

 **Not Supported**: Windows (fundamental rewrite required due to architectural differences(because of using libc), will be done when I finish the POSIX feature set, the API is also terribly complex compared to POSIX, there's also the fact that Windows has some amazing tools for this already,
 such as [`Everything`](https://www.voidtools.com/) )

### Testing methodology

I have 60+ Rust tests and 15+ correctness benchmarks run via shell for testing discrepancies against fd.

Note: I cannot validate my code with miri (Rust's )

The rust tests can be  [Found here](https://github.com/alexcu2718/fdf/blob/main/src/test.rs)

My shell scripts do clone the llvm repo (sorry!) to give an accurate testing environment

The rust tests are run via GitHub actions on the platforms supported.

To run the full test suite yourself:

```bash
git clone https://github.com/alexcu2718/fdf /tmp/fdf_test
cd /tmp/fdf_test/fd_benchmarks
./run_all_tests_USE_ME.sh
```

This runs a comprehensive suite of internal library, CLI tests, and benchmarks.

## Cool bits(full benchmarks can be seen in speed_benchmarks.txt)

 **Full Repeatable Benchmarks:** [Found at the following link](https://github.com/alexcu2718/fdf/blob/main/speed_benchmarks.txt)

(Repeatable via the testing code seen above, they cover file type filtering,extension, filesizes, among many more!)

Tests ran on my local system instead of the llvm-project (to give a good example)

```bash
Benchmark 1: fdf -HI '.*[0-9].*(md|\.c)$' '/home/alexc'
  Time (mean ± σ):     431.6 ms ±  10.6 ms    [User: 1307.7 ms, System: 3530.7 ms]
  Range (min … max):   414.7 ms … 446.1 ms    10 runs

Benchmark 2: fd -HI '.*[0-9].*(md|\.c)$' '/home/alexc'
  Time (mean ± σ):     636.7 ms ±  16.9 ms    [User: 3194.6 ms, System: 3780.9 ms]
  Range (min … max):   615.1 ms … 661.5 ms    10 runs

Summary
  fdf -HI '.*[0-9].*(md|\.c)$' '/home/alexc' ran
    1.48 ± 0.05 times faster than fd -HI '.*[0-9].*(md|\.c)$' '/home/alexc'

Benchmark 1: fdf '.' '/home/alexc' -HI
  Time (mean ± σ):     462.1 ms ±  17.8 ms    [User: 1233.5 ms, System: 3694.2 ms]
  Range (min … max):   432.4 ms … 491.3 ms    10 runs

Benchmark 2: fd '.' '/home/alexc' -HI
  Time (mean ± σ):     786.6 ms ±  19.2 ms    [User: 4548.4 ms, System: 3941.3 ms]
  Range (min … max):   743.8 ms … 808.5 ms    10 runs

Summary
  fdf '.' '/home/alexc' -HI ran
    1.70 ± 0.08 times faster than fd '.' '/home/alexc' -HI

Benchmark 1: fdf '.' '/home/alexc' -HI --type d
  Time (mean ± σ):     461.8 ms ±   7.4 ms    [User: 1109.2 ms, System: 3674.4 ms]
  Range (min … max):   451.3 ms … 473.2 ms    10 runs

Benchmark 2: fd '.' '/home/alexc' -HI --type d
  Time (mean ± σ):     681.2 ms ±  32.4 ms    [User: 3697.3 ms, System: 3784.3 ms]
  Range (min … max):   639.6 ms … 720.7 ms    10 runs

Summary
  fdf '.' '/home/alexc' -HI --type d ran
    1.48 ± 0.07 times faster than fd '.' '/home/alexc' -HI --type d


```

## Extra bits

-cstr! :a macro  use a byte slice as a pointer (automatically initialise memory(no heap use), then add a **null terminator** for FFI use)
(NOTE: THIS MAY CHANGE DUE TO IMPLICATIONS OF LLVM's probe-stacks (LLVM is the backend for compiling rust))

-find_char_in_word: Find the first occurrence of a byte in a 64-bit word (Using SWAR(SIMD within a register)), a const fn

-A black magic macro that can colour filepaths based on a compile time perfect hashmap
it's defined in another github repo of mine at this [found here](https://github.com/alexcu2718/compile_time_ls_colours)

Then this function, really nice way to avoid branch misses during dirent parsing (a really hot loop)

```rust
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;
// This has a computational complexity of  O(1)!, truly constant time and a constant function!
//The code is explained better in the true function definition
// I have to keep it short for the readme!)
//This is the little-endian implementation, see crate for modified version for big-endian
// Only used on Linux+Solaris+Illumos systems, OpenBSD/macos systems store the name length trivially
//(SIMD within a register, so no architecture dependence)
pub const unsafe fn dirent_const_time_strlen(dirent: *const dirent64) -> usize {
    //the only true unsafe action here is dereferencing the pointer, 
    //that MUST be checked before hand, hence why it's an unsafe function.
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; 
    //reclen is always multiple of 8 so alignment is guaranteed (unaligned reads are expensive!)
    //endianness fix omitted for brevity. check source
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch mask
    let candidate_pos = last_word | mask;//
    let byte_pos = 7 -  find_zero_byte_u64(candidate_pos) ; // no branch SWAR (this is a private function to prevent abuse)
    reclen - DIRENT_HEADER_START - byte_pos
}


```

## Why?

I started this project because I found find slow and wanted to learn how to interface directly with the kernel.
What began as a random experiment turned out to be a genuinely useful tool - one I'll probably use for the rest of my life, which is much more interesting than a project I'd just create and forget about.

At the core, this is about learning.

When I began I had barely used Linux for a few months, I didn't even know C, so there are some rough ABI edges. But along the way, I've picked up low-level skills and this project has been really useful for that!

### Performance Motivation

Even though fdf is already faster than fd in all cases, I'm experimenting with filtering before allocation(I don't stop at good enough!)
Rust's std::fs has some inefficiencies, too much heap allocation, file descriptor manipulation, constant strlen calculations, usage of readdir (not optimal because it implicitly stat calls every file it sees!). Rewriting all of it  using libc was the ideal way to bypass that and learn in the process.

Currently, filtering-before-allocation is partially implemented in the crate but not yet exposed via the CLI. If the results prove consistently performant, I'll integrate it into the public tool(I will probably leave this until I get datetime working sufficiently.)

### Development Philosophy

* Feature stability before breakage - I won't push breaking changes or advertise this anywhere until I've got a good baseline.

* Open to contributions - Once the codebase stabilises, I welcome others to add features if they're extremely inclined anyway!

* Pragmatic focus - Some areas, like datetime filtering, are especially complex and need a lot of investigation!

In short, this project is a personal exploration into performance, low-level programming, and building practical tools - with the side benefit that it's actually good at what it does.

## NECESSARY DISCLAIMERS

I've directly taken code from [fn-match, found at the link](https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574) and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for some SWAR tricks from the standard library [(see link)](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
I've found a much more rigorous way of doing some bit tricks via this.

I additionally emailed the author of memchr and got some nice tips, great guy, someone I respect whole heartedly!

I believe referencing similar work helps to aid in validating complex algorithms!

## Future Plans

### Modularisation

While avoiding excessive fragmentation, I plan to extract reusable components (like platform-specific FFI utilities) into separate crates. This will improve maintainability without sacrificing the project's cohesive design.

### Feature Enhancements (Planned)

**DateTime Filtering**: Fast, attribute-based file filtering by time (high priority despite personal infrequent use, I have a lot of test cases to attempt  this, it's also complex to reproduce the time methodologies for all POSIX platforms because each one differs so much, the drawbacks of not using the stdlib!)

**Extended File Types**: Support for searching device drivers, and other special files(this isn't difficult at all, just not a priority)

**POSIX Compliance**: Mostly done, I don't expect to extend this beyond Linux/BSD/MacOS/Illumos/Solaris (the other ones are embedded mostly, correct me if i'm wrong!)

### Platform Expansion

**Windows Support**: Acknowledged as a significant undertaking an almost entire separate codebase(portability ain't fun), but valuable for both usability and learning Windows internals.

## Enhance shell completions

Meanwhile my shell completions are pretty good, I want to improve them a lot, this shouldn't be too bad.

### Core Philosophy

The CLI will remain **simple** (avoiding overwhelming help menus(looking at you, ripgrep!)) and **efficient** (prioritising performance in both design and implementation).

## Current Issues

There's bugs I need to diagnose causing small differences when doing size difference, [check/run this script](https://github.com/alexcu2718/fdf/blob/main/test_size_difference.sh), I'm pretty sure this is actually a bug in fd, going to investigate before pointing fingers!

## Installation and Usage

```bash
# Clone & build
git clone https://github.com/alexcu2718/fdf.git
cd fdf
cargo build --release

# Optional system install
cargo install --git https://github.com/alexcu2718/fdf


# Find all JPG files in the home directory (excluding hidden files)
fdf . ~ -e jpg

# Find all  Python files in /usr/local (including hidden files)
fdf . /usr/local -e py -H


# Generate shell completions for Zsh/bash (also supports powershell/fish!)
# For Zsh
echo 'eval "$(fdf --generate zsh)"' >> ~/.zshrc

# For Bash
echo 'eval "$(fdf --generate bash)"' >> ~/.bashrc

## Options 
Usage: fdf [OPTIONS] [PATTERN] [PATH]

Arguments:
  [PATTERN]
          Pattern to search for

  [PATH]
          Path to search (defaults to current working directory)

Options:
  -H, --hidden
          Shows hidden files eg .gitignore or .bashrc, defaults to off

  -s, --case-sensitive
          Enable case-sensitive matching, defaults to false

  -e, --extension <EXTENSION>
          filters based on extension, eg --extension .txt or -E txt

  -j, --threads <THREAD_NUM>
          Number of threads to use, defaults to available threads available on your computer

          [default: <NUM_CORES>]

  -a, --absolute-path
          Show absolute paths of results, defaults to false

  -I, --include-dirs
          Include directories, defaults to off

  -L, --follow
          Include symlinks in traversal,defaults to false

      --nocolour
          Disable colouring output when sending to terminal

  -g, --glob
          Use a glob pattern,defaults to off

  -n, --max-results <TOP_N>
          Retrieves the first eg 10 results, '.cache' / -n 10

  -d, --depth <DEPTH>
          Retrieves only traverse to x depth

      --generate <GENERATE>
          Generate shell completions

          [possible values: bash, elvish, fish, powershell, zsh]

  -t, --type <TYPE_OF>...
          Select type of files (can use multiple times).
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

  -p, --full-path
          Use a full path for regex matching, default to false

  -F, --fixed-strings
          Use a fixed string not a regex, defaults to false

  -S, --size <size>
          Filter by file size

          PREFIXES:
            +SIZE    Find files larger than SIZE
            -SIZE    Find files smaller than SIZE
             SIZE     Find files exactly SIZE (default)

          UNITS:
            b        Bytes (default if no unit specified)
            k, kb    Kilobytes (1000 bytes)
            ki, kib  Kibibytes (1024 bytes)
            m, mb    Megabytes (1000^2 bytes)
            mi, mib  Mebibytes (1024^2 bytes)
            g, gb    Gigabytes (1000^3 bytes)
            gi, gib  Gibibytes (1024^3 bytes)
            t, tb    Terabytes (1000^4 bytes)
            ti, tib  Tebibytes (1024^4 bytes)

          EXAMPLES:
            --size 100         Files exactly 100 bytes
            --size +1k         Files larger than 1000 bytes
            --size -10mb       Files smaller than 10 megabytes
            --size +1gi        Files larger than 1 gibibyte
            --size 500ki       Files exactly 500 kibibytes

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### Potential Future Enhancements

**1. Custom Arena Allocator**  
-- Investigate implementing from scratch  
-- Reference implementation: [Microsoft's Edit Arena](https://github.com/microsoft/edit/tree/main/src/arena)  
-- (Caveat: see comments at <https://github.com/microsoft/edit/blob/main/src/arena/release.rs>,
-- this essentially says that my custom allocator (MiMalloc) is *just* as fast, I think it'd be interesting to test this!)

**2. io_uring System Call Batching**  
-- Explore batched stat operations (and others as appropriate)  
-- Significant challenges:  
-- Current lack of `getdents` support in io_uring  
-- Necessitates async runtime integration (potential Tokio dependency)  
-- Conflicts with minimal-dependency philosophy  
-- Linux only, that isn't too appealing for such a difficult addition (I'll probably not do it)

**3. Native Threading Implementation**  
-- Replace Rayon dependency  
-- Develop custom work-distribution algorithm  
-- Current status: Experimental approaches underway  

**4. Allocation-Optimised Iterator Adaptor**  
-- Design filter mechanism avoiding:  
-- Unnecessary directory allocations  
-- Non-essential memory operations  

**5. MacOS/BSD(s(potentially) Specific Optimisations**
-- Implement an iterator using getattrlistbulk (this may be possible for bsd too? or perhaps just linking getdirentries for BSD systems)
-- Test repo found at <https://github.com/alexcu2718/mac_os_getattrlistbulk_ls>
-- This allows for much more efficient syscalls to get filesystem entries
-- (Admittedly I've been a bit hesitant about this, because the API is quite complex and unwieldy!)
