# fdf – High-Performance POSIX File Finder

**fdf** is an experimental, high-performance alternative to [`fd`](https://github.com/sharkdp/fd) and `find`, optimised for **regex** and **glob** matching with colourised output.  
Originally a learning project in **advanced Rust**, **C**, and **assembly**, it has evolved into a competitive, benchmarked tool for fast filesystem search.

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](<https://github.com/alexcu2718/fdf/actions>)

Easily installed via:

```bash
cargo install --git https://github.com/alexcu2718/fdf
```

## Important Notes

Contributions will be considered once features are stabilised and improved. This remains a learning/hobby project requiring significant development.

(Although if someone really wants to contribute, go nuts!)

The implemented subset performs well, surpassing fd in equivalent feature sets, though fd offers a broader range. The project focuses on exploring hardware-specific code optimisation rather than replicating fd's full functionality. Ultimately I wanted a really fast regex/glob tool for myself and learning how to program at a low level.

## Platform Support Status (64 bit only)

 **Fully Supported & CI Tested**: Linux (x86_64, aarch64, s390x, RISC-V64), macOS (Intel & Apple Silicon), FreeBSD(x86_64)

 **Compiles but Limited Testing**: OpenBSD, NetBSD,  DragonflyBSD, Android  (Not familiar with github actions for these ones)

 **Not Supported**: Windows (fundamental rewrite required due to architectural differences, will be done when I read through the API properly!)

## How to test

```bash
git clone https://github.com/alexcu2718/fdf /tmp/fdf_test  &&   /tmp/fdf_test/fd_benchmarks/run_all_tests_USE_ME.sh
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

-find_char_in_word: Find the first occurrence of a byte in a 64-bit word (Using SWAR(SIMD within a register))

-A black magic macro that can colour filepaths based on a compile time perfect hashmap
it's defined in another github repo of mine at <https://github.com/alexcu2718/compile_time_ls_colours>

Then this function, really nice way to avoid branch misses during dirent parsing (a really hot loop)

```rust

//The code is explained better in the true function definition (this is crate agnostic)
//This is the little-endian implementation, see crate for modified version for big-endian
// Only used on Linux systems, OpenBSD/macos systems store the name length trivially (no clue on Windows because reading the API is AWFUL)
use fdf::find_zero_byte_u64; // a const SWAR function 
//(SIMD within a register, so no architecture dependence)
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    //the only true unsafe action here is dereferencing the pointer, that MUST be checked before hand
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; //reclen is always multiple of 8 so alignment is guaranteed (unaligned reads are expensive!)
    //endianness fix omitted for brevity. check source
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch
    let candidate_pos = last_word | mask;//^
    let byte_pos = 7 -  find_zero_byte_u64(candidate_pos) ; // no branch SWAR
    reclen - DIRENT_HEADER_START - byte_pos
}


```

## WHY?

Well, I found find slow, and I wanted to learn about how to interface directly with the kernel, I didn't expect some random test project to actually be good.

Then finally, the reward is a tool I can use for the rest of my life to find stuff.

Mostly though, I just enjoy learning.

To put it in perspective, I did not know any C before I started this project, so there are rough ABI bits.

Even though my project in it's current state is faster, I've got some experiments to try filtering before allocating.

Rust's std::fs has some notable inefficiencies in how it works, notably a lot more heap allocation than I'd like so rewriting from libc was the ideal way to bypass this(and learn!)

I'm curious to see what happens when you filter before allocation, this is something I have partially working in my current crate
but the implementation details like that is not accessible via CLI. If it proves to be performant, it will eventually be in there.
Obviously, I'm having to learn a lot to do these things and it takes  TIME to understand, get inspired and implement things...

I do intend to only add features and not break anything, until I can somewhat promise that, then i won't entertain wasting other people's time but eventually
 if anyone felt like adding something, they can!

(notably, there's some obvious things I have not touched in a while(datetime filters) and things that are just less interesting, ideally one day someone could do that, not now though)

## NECESSARY DISCLAIMERS (I might have a conscience somewhere)

I've directly taken code from <https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574> and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for here <https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161>
I've found a much more rigorous way of doing some bit tricks via this

I enjoy relying on  validated work like stdlib to ideally 'covalidate' my work, aka less leaps of logic required to make the assessment

## Future plans?

Separation of utilities

Right now, it's a bit monolithic. Some aspects might deserve their own crate (i dislike the idea of having 500 crates to do 1 specific thing each)
(Although, writing FFI like this for multiple different POSIX systems with distinct pecularities will tend to be a lot of code)

I'd probably just keep the CLI stuff simple, features to be added are datetime based filtering (could be done quick, I just have rarely used time based filtering and that's why it's slow!) as well as just other things, eg to search for device drivers/etc.

Add POSIX compatibility in general ( illumos/solaris QEMU isn't straight forward, quite esoteric)

Add Windows... Well, This would take a fundamental rewrite because of architectural differences, I might do it.
(It may be interesting to learn the differences actually)

Additional features on my compile_time_ls_colours would be nice, I think I want to explore compile time hashmaps more,
I'm only really scratching the service on metaprogramming and rust's utilities are great (one day I'll try cpp template metaprogramming and become a convert...)

Fundamentally I want to develop something that's simple to use (doing --help shouldnt give you the bible)
..and exceedingly efficient.

## COMPATIBILITY STATE

### Automatically Tested via GitHub Actions CI/CD

The following platforms are continuously tested on every commit and pull request:

- **Linux x86_64** - Ubuntu latest (glibc)
- **Linux aarch64** - Ubuntu 22.04 (cross-compiled)
- **Linux s390x** - Ubuntu 22.04 (big-endian architecture)
- **Linux RISC-V64** - Ubuntu 22.04 (emerging architecture)
- **macOS x86_64** - Latest macOS runner
- **macOS Apple Silicon (aarch64)** - macOS 14
- **FreeBSD x86_64** - Using VM testing environment

### Additional Verified Platforms

Beyond CI testing, fdf has been manually verified on:

- **Linux distributions**: Debian, Ubuntu, Arch, Fedora (various versions)
- **Linux architectures**: MUSL static linking supported
- **Android**: aarch64 Debian environment
- **BSD systems**: OpenBSD, NetBSD, DragonflyBSD (compiles but limited testing)
- **Big-endian systems**: Ubuntu PPC64 (compilation confirmed, 20+ minute build time)

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


Options: aults to off

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

TODO LIST (Maybe):

-- Arena Allocator (potentially): Written from scratch. See Microsoft's edit for a nice example:
   <https://github.com/microsoft/edit/tree/main/src/arena>

-- io_uring for Batched Syscalls: e.g., batched open/read operations.
   This will be extremely challenging.

   Unfortunately uring lacks the op code required for getdents-- however
   other op codes are available, but this would require a LOT of work,

   it also would require an async runtime->
   Which inevitably means tokio->
  which means most of my work in avoiding dependencies goes down the bin
   (I'm already unhappy being reliant on rayon but that's on the list to remove.)

-- I might continue developing my compile time hashmap for LS_COLORS and make an easier way to do these maps, it's got a good general use case and the macro use is pretty fun!
   However I do have a separate commit at <https://github.com/alexcu2718/compile_time_ls_colours/tree/no_phf_build>
   Which has no dependencies, although it's annoying to do without doing a HELLA lot of byte manipulation yourself.
   (also, it's runtime statically initialised, not as cool!)

-- Threading Without Rayon: My attempts have come close, but aren’t quite there yet.
   I'll rely on Rayon for now until I can come up with a smart way to implement an appropriate work-distributing algorithm. TODO!

-- Iterator Adaptor + Filter: Some kind of adaptor that avoids a lot of unnecessary allocations on non-directories.

-- Syscall Limits: I think there’s ultimately a hard limit on syscalls.
   I've experimented with an early Zig `io_uring + getdents` implementation — but it's well outside my comfort zone (A LOT).
   I’ll probably give it a go anyway (if possible).

**** THIS IS NOT FINISHED. I have no idea what the long-term plans are — I'm just trying to make stuff go fast and learn, OK?
