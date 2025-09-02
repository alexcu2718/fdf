# fdf - High-Performance POSIX File Finder

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](https://github.com/alexcu2718/fdf/actions)

fdf is a high-performance POSIX file finder written in Rust with extensive C FFI and assembly optimisation. It serves as a lightweight alternative to tools such as fd and find, with a focus on speed, efficiency, and cross-platform compatibility. Benchmarks demonstrate fdf running up to 2x faster than comparable tools, achieved through low-level optimisation, SIMD techniques, and direct kernel interfacing.

**Quick Installation:**

```bash
cargo install --git https://github.com/alexcu2718/fdf
```

## Project Status

This is primarily a learning and performance exploration project. Whilst already useful and performant, it remains under active development towards a stable 1.0 release. The name 'fdf' is a temporary placeholder.

The implemented subset performs exceptionally well, surpassing fd in equivalent feature sets, though fd offers broader functionality. This project focuses on exploring hardware-specific code optimisation rather than replicating fd's complete feature set.

## Platform Support (64-bit only)

### Fully Supported and CI Tested

- Linux (x86_64, aarch64, s390x, RISC-V64, Alpine MUSL)
- macOS (Intel and Apple Silicon)
- FreeBSD (x86_64)

### Compiles with Limited Testing

- OpenBSD, NetBSD, DragonflyBSD (tested occasionally, minor fixes expected if issues arise)
- Android (tested on device)
- Illumos and Solaris (x86_64, verified with QEMU)

### Not Yet Supported

- **Windows**: Requires significant rewrite due to architectural differences with libc. Planned once the POSIX feature set is stable. Windows already has highly effective tools such as [Everything](https://www.voidtools.com/).

*Note: GitHub Actions does not yet provide Rust 2024 support for some platforms. Additional checks will be added when available.*

## Testing

The project includes comprehensive testing with 70+ Rust tests and 15+ correctness benchmarks comparing against fd.

Note: Miri validation (Rust's undefined behaviour detector) cannot be used due to the extensive libc calls and assembly code. Intensive testing and valgrind validation are used instead.

- Rust tests: [Available here](https://github.com/alexcu2718/fdf/blob/main/src/test.rs)
- Shell scripts clone the LLVM repository to provide an accurate testing environment
- Tests run via GitHub Actions on all supported platforms

**Running the Full Test Suite:**

```bash
git clone https://github.com/alexcu2718/fdf /tmp/fdf_test
cd /tmp/fdf_test/fd_benchmarks
./run_all_tests_USE_ME.sh
```

This executes a comprehensive suite of internal library tests, CLI tests, and benchmarks.

## Performance Benchmarks

**Complete benchmarks:** [Available here](https://github.com/alexcu2718/fdf/blob/main/speed_benchmarks.txt)

The benchmarks are fully repeatable using the testing code above and cover file type filtering, extension matching, file sizes, and many other scenarios. The following results were obtained on a local system (rather than the LLVM project) to provide realistic usage examples:
(These are tests done via hyperfine and summarised to save space here.)

| Test Case | fdf Time | fd Time | Speedup |
|-----------|----------|---------|---------|
| Regex pattern matching | 431.6ms | 636.7ms | 1.48x faster |
| Files >1MB | 896.9ms | 1.732s | 1.93x faster |
| General search | - | - | 1.70x faster |
| Directory filtering | 461.8ms | 681.2ms | 1.48x faster |

## Technical Highlights

### Key Optimisations

- **cstr! macro**: Uses a byte slice as a pointer, automatically initialising memory (no heap allocation) and adding a null terminator for FFI use
- **find_char_in_word**: Locates the first occurrence of a byte in a 64-bit word using SWAR (SIMD within a register), implemented as a const function
- **Compile-time colour mapping**: A compile-time perfect hashmap for colouring file paths, defined in a [separate repository](https://github.com/alexcu2718/compile_time_ls_colours)

### Constant-Time Directory Entry Processing

The following function provides an elegant solution to avoid branch mispredictions during directory entry parsing (a performance-critical loop):

```rust
#[cfg(not(target_os = "linux"))]
use libc::dirent as dirent64;
#[cfg(target_os = "linux")]
use libc::dirent64;

// Computational complexity: O(1) - truly constant time
// This is the little-endian implementation; see source for big-endian version
// Used on Linux/Solaris/Illumos systems; OpenBSD/macOS store name length trivially
// SIMD within a register, so no architecture dependence
#[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))] 
pub const unsafe fn dirent_const_time_strlen(dirent: *const dirent64) -> usize {
    // The only unsafe action is dereferencing the pointer
    // This MUST be validated beforehand
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; 
    // reclen is always multiple of 8 so alignment is guaranteed
    let mask = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // branchless mask
    let candidate_pos = last_word | mask;
    let byte_pos = 7 - find_zero_byte_u64(candidate_pos); // branchless SWAR
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

** Feature stability before breakage - I won't push breaking changes or advertise this anywhere until I've got a good baseline.

** Open to contributions - Once the codebase stabilises, I welcome others to add features if they're extremely inclined anyway!

** Pragmatic focus - Some areas, like datetime filtering, are especially complex and need a lot of investigation!

In short, this project is a personal exploration into performance, low-level programming, and building practical tools - with the side benefit that it's actually good at what it does.

## Acknowledgements/Disclaimers

I've directly taken code from [fnnmatch-regex, found at the link](https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574) and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
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

Meanwhile my shell completions are pretty good, I want to improve them a lot, this shouldn't be too bad(famous last words)

### Core Philosophy

The CLI will remain **simple** (avoiding overwhelming help menus(looking at you, ripgrep!)) and **efficient** (prioritising performance in both design and implementation).

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

          [default: <MAX_NUM_THREADS>]

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

  -t, --type <TYPE_OF>
          Select type of files.
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
