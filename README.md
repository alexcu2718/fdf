# fdf - High-Performance POSIX File Finder

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](https://github.com/alexcu2718/fdf/actions)

fdf is a high-performance POSIX file finder written in Rust with extensive C FFI. It serves as a lightweight alternative to tools such as fd and find, with a focus on speed, efficiency, and cross-platform compatibility. Benchmarks demonstrate fdf running up to 2x faster than comparable tools, achieved through low-level optimisation, SIMD techniques, and direct kernel interfacing.

PLEASE NOTE: This is due to undergo a rename before a 1.0

**Quick Installation:**

```bash
cargo install --git https://github.com/alexcu2718/fdf
```

## Project Status

This is primarily a learning and performance exploration project. Whilst already useful and performant, it remains under active development towards a stable 1.0 release. The name 'fdf' is a temporary placeholder.

The implemented subset performs exceptionally well, surpassing fd in equivalent feature sets, though fd offers broader functionality. This project focuses on exploring hardware-specific code optimisation rather than replicating fd's complete feature set.

While the CLI is usable, the internal library is NOT suggested for use

## Platform Support (64-bit only)

### Fully Supported and CI Tested

- Linux (x86_64, aarch64, s390x, RISC-V64, Alpine MUSL)
- macOS (Intel and Apple Silicon)
- FreeBSD (x86_64)

### Compiles with Limited Testing

*Note: GitHub Actions does not yet provide Rust 2024 support for some(most of these) platforms. Additional checks will be added when available.*

- OpenBSD, NetBSD, DragonflyBSD (tested occasionally, minor fixes expected if issues arise)
- Android (tested on device)
- Illumos and Solaris (x86_64, verified with QEMU)

### Not Yet Supported

- **Windows**: Requires significant rewrite due to architectural differences with libc. Planned once the POSIX feature set is stable. Windows already has highly effective tools such as [Everything](https://www.voidtools.com/). The plan is this to work on this after a 1.0.

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

The benchmarks are fully repeatable using the testing code above and cover file type filtering, extension matching, file sizes, and many other scenarios. The following results were obtained on a local system and the LLVM repo to provide realistic usage examples:
(These are tests done via hyperfine and summarised to save space here.)

(*TESTED ON LINUX, other OS's will (probably) be lower due to specific linux optimisations)

| Test Case                              | Files Found | fdf Time (mean) | fd Time (mean) | Speedup (×) | Notes |
|---------------------------------------|--------------|-----------------|----------------|--------------|--------|
| Depth-limited (depth=2, LLVM)         | 396          | 9.6 ms          | 18.5 ms        | 1.93 ± 0.40  | No differences |
| File extension (.c, LLVM)             | 12,801       | 21.1 ms         | 36.5 ms        | 1.73 ± 0.29  | No differences |
| No pattern (LLVM)                     | 176,841      | 25.6 ms         | 37.9 ms        | 1.48 ± 0.14  | No differences |
| Relative directory (..)               | 198,933      | 29.1 ms         | 41.4 ms        | 1.42 ± 0.31  | No differences |
| Regex pattern (LLVM)                  | 4,439        | 22.5 ms         | 34.5 ms        | 1.53 ± 0.20  | No differences |
| Size >1MB (LLVM)                      | 118          | 34.9 ms         | 75.9 ms        | 2.18 ± 0.15  | No differences |
| Type filter (directory)               | 15,224       | 21.9 ms         | 35.6 ms        | 1.63 ± 0.35  | No differences |
| Type filter (empty)                   | 2,843        | 39.8 ms         | 62.9 ms        | 1.58 ± 0.22  | No differences |
| Type filter (executable)              | 929          | 32.3 ms         | 50.8 ms        | 1.57 ± 0.14  | No differences |
| Cold cache regex (LLVM)               | —            | 25.5 ms         | 46.7 ms        | 1.83 ± 0.16  | No differences |
| Depth-limited (depth=4, home dir)     | 54,544       | 18.2 ms         | 27.4 ms        | 1.51 ± 0.41  | No differences |
| File extension (.c, home dir)         | 96,910       | 295.9 ms        | 606.7 ms       | 2.05 ± 0.05  | No differences |
| No pattern (home dir)                 | 2,216,706    | 360.9 ms        | 653.8 ms       | 1.81 ± 0.04  | No differences |
| Regex pattern (home dir)              | 69,964       | 331.2 ms        | 544.6 ms       | 1.64 ± 0.04  | No differences |
| Size >1MB (home dir)                  | 12,097       | 893.5 ms        | 2.089 s        | 2.34 ± 0.54  | No differences |
| Size <1MB (home dir)                  | 1,968,643    | 1.007 s         | 1.866 s        | 1.85 ± 0.13  | No differences |

**Overall:**  
Across all benchmarks, `fdf` consistently outperformed `fd`, producing identical results in every test.

**Average speedup:** **1.74× faster**

## Distinctions from fd/find

Symlink resolution in my method differs from fd and find. Although I generally advise against following symlinks, the option exists for completeness.

When following symlinks, behaviour will vary slightly. For example, fd can enter infinite loops with recursive symlinks
 (see recursive_symlink_fs_test.sh) [Available here](https://github.com/alexcu2718/fdf/blob/main/recursive_symlink_fs_test.sh)
whereas my implementation prevents hangs. It may, however, return more results than expected.

To avoid issues, use --same-file-system when traversing symlinks. Both fd and find also handle them poorly without such flags. My approach ensures the program always terminates safely, even in complex directories like ~/.steam, ~/.wine, /sys, and /proc.

## Technical Highlights

### Key Optimisations

-**Getdents: Optimised the Linux-specific directory reading by significantly reducing the number of getdents system calls.  This approach enables single-pass reads for small directories and reduces getdents invocations by roughly 50% in testing. See the skip code [at this link](https://github.com/alexcu2718/fdf/blob/3fc7c2c13ec62e9004409e21dcd7c5ce0a31b438/src/iter.rs#L515)

- **find_char_in_word**: Locates the first occurrence of a byte in a 64-bit word using SWAR (SIMD within a register), implemented as a const function

- **Compile-time colour mapping**: A compile-time perfect hashmap for colouring file paths, defined in a [separate repository](https://github.com/alexcu2718/compile_time_ls_colours)

### Constant-Time Directory Entry Processing

The following function provides an elegant solution to avoid branch mispredictions/SIMD instructions during directory entry parsing (a performance-critical loop):

See source for bigendian/original version [found here](https://github.com/alexcu2718/fdf/blob/3fc7c2c13ec62e9004409e21dcd7c5ce0a31b438/src/utils.rs#L180)

```rust
// Computational complexity: O(1) - truly constant time
// This is the little-endian implementation; see source for big-endian version(with better explanations!) 
// Used on Linux/Solaris/Illumos systems; OpenBSD/macOS store name length trivially
// SIMD within a register, so no architecture dependence
#[cfg(any(target_os = "linux", target_os = "illumos", target_os = "solaris"))] 
pub const unsafe fn dirent_const_time_strlen(dirent: *const dirent64) -> usize {
    // The only unsafe action is dereferencing the pointer
    // This MUST be validated beforehand
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(dirent64, d_name) ;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; 
    // reclen is always multiple of 8 so alignment is guaranteed
    let mask = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // branchless mask
    let candidate_pos = last_word | mask;
    let byte_pos = 8 - find_zero_byte_u64(candidate_pos); // branchless SWAR
    reclen - DIRENT_HEADER_START - byte_pos
}


```

## Why?

I started this project because I found find slow and wanted to learn how to interface directly with the kernel.
What began as a random experiment turned out to be a genuinely useful tool - one I'll probably use for the rest of my life, which is much more interesting than a project I'd just create and forget about.

At the core, this is about learning.

When I began I had barely used Linux/Rust for a few months, I didn't even know C, so there are some rough ABI edges. But along the way, I've picked up low-level skills and this project has been really useful for that!

### Performance Motivation

Even though fdf is already faster than fd in all cases, I'm planning to experiment with filtering before allocation(I don't stop at good enough!)
Rust's std::fs has some inefficiencies, too much heap allocation, file descriptor manipulation, constant strlen calculations, usage of readdir (not optimal because it implicitly stat calls every file it sees!). Rewriting all of it  using libc was the ideal way to bypass that and learn in the process.

Notably the standard library will keep file descriptors open(UNIX specific) until the last reference to the inner `ReadDir` disappears,
fundamentally this means that this cause a lot more IO. It will also tend to call 'stat' style calls heavily which seemed inefficient
(I do have a shell script documenting syscall differences here(it's crude but it works well)) [Available here](https://github.com/alexcu2718/fdf/blob/main/fd_benchmarks/syscalltest.sh)

### Development Philosophy

** Feature stability before breakage - I won't push breaking changes or advertise this anywhere until I've got a good baseline.

** Open to contributions - Once the codebase stabilises, I welcome others to add features if they're extremely inclined anyway!

** Pragmatic focus - Some areas, like datetime filtering, are especially complex and need a lot of investigation!

In short, this project is a personal exploration into performance, low-level programming, and building practical tools - with the side benefit of making a useful tool and learning a crazy amount!

## Acknowledgements/Disclaimers

I've directly taken code from [fnmatch-regex, found at the link](https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574) and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for some SWAR tricks from the standard library [(see link)](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
I've found a much more rigorous way of doing some bit tricks via this.

I additionally emailed the author of memchr and got some nice tips, great guy, someone I respect whole heartedly!

## Future Plans

### Modularisation

While avoiding excessive fragmentation, I plan to extract reusable components (like platform-specific FFI utilities) into separate crates. This will improve maintainability without sacrificing the project's cohesive design.

### Feature Enhancements (Planned)

** API cleanup, currently the CLI is the main focus but I'd like to fix that eventually!

**DateTime Filtering**: Fast, attribute-based file filtering by time (high priority despite personal infrequent use, I have a lot of test cases to attempt  this, admittedly I've been focusing on tidying up the API a lot)

**POSIX Compliance**: Mostly done, I don't expect to extend this beyond Linux/BSD/MacOS/Illumos/Solaris (the other ones are embedded mostly, correct me if i'm wrong!)

### Platform Expansion

**Windows Support**: Acknowledged as a significant undertaking an almost entire separate codebase(portability ain't fun), but valuable for both usability and learning Windows internals.

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

  -p, --full-path
          Use a full path for regex matching, default to false

  -F, --fixed-strings
          Use a fixed string not a regex, defaults to false

      --show-errors
          Show errors when traversing

      --same-file-system
          Only traverse the same filesystem as the starting directory

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

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### Potential Future Enhancements

#### 1. io_uring System Call Batching  

- Investigate batching of `stat` and similar operations.  
- **Key challenges:**  
  - No native `getdents` support in `io_uring`.  
  - Would require async runtime integration (e.g. Tokio).  
  - Conflicts with the project’s minimal-dependency design.  
  - Linux-only feature, making it a low-priority and high-effort addition.  

#### 2. Native Threading Implementation  

- Replace the Rayon dependency with a custom threading model.
- Develop a bespoke work-distribution algorithm.  
- **Status:** Experimental work in progress.  

#### 3. Allocation-Optimised Iterator Adaptor  

- Implement a filtering mechanism that avoids unnecessary directory allocations.  
- Achieved via a closure-based approach triggered during `readdir` or `getdents` calls.  

#### 4. macOS/*BSD-Specific Optimisations  

- Explore using `getattrlistbulk` on macOS (and possibly `getdirentries` on BSD).  
- **Test repository:** [mac_os_getattrlistbulk_ls](https://github.com/alexcu2718/mac_os_getattrlistbulk_ls).  
- Enables more efficient filesystem entry retrieval.  
- Currently low priority due to API complexity and limited portability.  

#### 5. Solaris / Illumos / Android Optimisations  

- Although `getdents` is Linux-specific, other systems (Android, Solaris, QEMU) expose compatible libc syscalls.  
- These could be integrated with minimal effort using existing code infrastructure.  
- Implementation would likely take only a few hours once prioritised.  
