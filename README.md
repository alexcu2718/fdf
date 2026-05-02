# fdf - High-Performance POSIX File Finder

[![CI (main)](https://github.com/alexcu2718/fdf/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/alexcu2718/fdf/actions/workflows/rust.yml?query=branch%3Amain)

fdf is a high-performance POSIX file finder written in Rust with extensive C FFI.

It serves as a lightweight alternative to tools such as fd and find, with a focus on speed, efficiency, and cross-platform compatibility. Benchmarks demonstrate fdf running up to 2x faster than comparable tools, achieved through low-level optimisation, SIMD techniques, and  direct syscalls(where possible).

Note, my philosophy is to keep this non-publicised at at all until a 1.0.

PLEASE NOTE: This is due to undergo a rename before a 1.0, I am tending towards `frep` as a name

**Quick Installation:**

```bash
cargo install --git https://github.com/alexcu2718/fdf

## Additionally specify  --no-default-features to remove mimalloc dependency
```

## Project Status

This is a performance-focused project that remains under active development towards a stable 1.0 release. The current name is temporary and will change before that release.

The CLI is already usable, but the internal library API is not yet stable.

## Platform Support

### Fully Supported and CI Tested

- Linux (x86_64, s390x (Big endian), Alpine( MUSL libc))
- macOS (Intel and Apple Silicon)
- FreeBSD (x86_64)
- NetBSD (x86_64)
- OpenBSD (x86_64)
- Solaris/Illumos(x86_64)

### Compiles with Limited Testing

*Note: GitHub Actions does not yet provide Rust 2024 support for some of these platforms. Additional checks will be added when available.*

- Android
- 32-bit Linux

Other POSIX operating systems, such as AIX, are currently untested.

### Not Yet Supported

- **Windows**: Requires significant rewrite due to architectural differences with libc. Planned once the POSIX feature set is stable.

- **DragonflyBSD**: Blocked on Rust 2024 support.

## Testing

The project includes comprehensive testing with 100+ Rust tests and 15+ correctness benchmarks comparing against fd.

Miri validation (Rust's undefined behaviour detector) is not practical here due to the extensive libc usage, so validation relies on intensive testing and Valgrind. See [scripts/valgrind-test.sh](./scripts/valgrind-test.sh).

- Rust tests: [Available here](./src/test.rs)
- Shell scripts clone the LLVM repository to provide an accurate testing environment
- Tests run via GitHub Actions on all supported platforms

**Running the Full Test Suite:**

```bash

TMP_DIR="${TMP:-/tmp}"
git clone --depth 1 https://github.com/alexcu2718/fdf "$TMP_DIR/fdf_test"
cd "$TMP_DIR/fdf_test"

./scripts/run_benchmarks.sh
```

This runs the internal library tests, CLI tests, and benchmarks.

## Performance Benchmarks

The benchmarks are repeatable using the testing code above and cover file type filtering, extension matching, file sizes, and several other scenarios. The following results were gathered on Linux against local directories and the LLVM repository and summarised from hyperfine output.

```bash
| Test Case                                                              | fdf Mean        | fd Mean         | Speedup   | Relative        |
| :----------                                                            | :--------:      | :-------:       | :-------: | :--------:      |
| cold-cache `.' '/home/alexc' -HI -d 4`                                 | 244.1 ± 10.3    | 353.6 ± 5.1     | 1.45x     | 1.45 ± 0.06     |
| cold-cache `.' '/tmp/llvm-project' -HI -d 2`                           | 14.0 ± 0.2      | 72.9 ± 1.1      | 5.21x     | 5.22 ± 0.12     |
| cold-cache `-HI --extension 'c' '' '/home/alexc`                       | 5.752 ± 1.169   | 5.852 ± 1.416   | 1.02x     | 1.02 ± 0.32     |
| cold-cache `-HI --extension 'c' '' '/tmp/llvm-project`                 | 28.6 ± 2.9      | 99.4 ± 2.8      | 3.48x     | 3.47 ± 0.36     |
| cold-cache `.' '/home/alexc' -HI`                                      | 4.413 ± 0.031   | 4.594 ± 0.040   | 1.04x     | 1.04 ± 0.01     |
| cold-cache `.' '/tmp/llvm-project' -HI`                                | 29.3 ± 0.6      | 100.1 ± 2.3     | 3.42x     | 3.41 ± 0.11     |
| cold-cache `.' '..' -HI`                                               | 36.5 ± 0.8      | 117.3 ± 2.5     | 3.21x     | 3.22 ± 0.10     |
| cold-cache `-HI '.*[0-9].*(md\|\.c)$' '/home/alexc`                    | 5.696 ± 1.386   | 7.258 ± 0.632   | 1.27x     | 1.27 ± 0.33     |
| cold-cache `-HI '.*[0-9].*(md\|\.c)$' '/tmp/llvm-project`              | 31.4 ± 0.5      | 103.9 ± 3.5     | 3.31x     | 3.31 ± 0.13     |
| cold-cache `-HI --size +1mb '' '/home/alexc`                           | 5.632 ± 0.015   | 5.982 ± 0.014   | 1.06x     | 1.06 ± 0.00     |
| cold-cache `-HI --size '-1mb' '' '/tmp/llvm-project`                   | 55.0 ± 1.9      | 148.3 ± 3.4     | 2.70x     | 2.70 ± 0.11     |
| cold-cache `-HI --size -1mb '' '/home/alexc`                           | 5.558 ± 0.007   | 6.183 ± 0.272   | 1.11x     | 1.11 ± 0.05     |
| cold-cache `.' '/tmp/llvm-project' -HI --type d`                       | 30.1 ± 0.6      | 106.3 ± 1.4     | 3.53x     | 3.53 ± 0.08     |
| cold-cache `.' '/tmp/llvm-project' -HI --type e`                       | 83.8 ± 5.7      | 168.7 ± 2.5     | 2.01x     | 2.01 ± 0.14     |
| cold-cache `.' '/tmp/llvm-project' -HI --type x`                       | 67.0 ± 2.7      | 144.0 ± 1.5     | 2.15x     | 2.15 ± 0.09     |
| warm-cache `.' '/home/alexc' -HI -d 4`                                 | 8.2 ± 0.4       | 21.0 ± 0.6      | 2.56x     | 2.57 ± 0.15     |
| warm-cache `.' '/tmp/llvm-project' -HI -d 2`                           | 2.5 ± 0.3       | 5.7 ± 0.5       | 2.28x     | 2.25 ± 0.32     |
| warm-cache `-HI --extension 'c' '' '/home/alexc`                       | 108.4 ± 1.2     | 191.3 ± 1.6     | 1.76x     | 1.76 ± 0.02     |
| warm-cache `-HI --extension 'c' '' '/tmp/llvm-project`                 | 16.0 ± 0.8      | 31.4 ± 1.1      | 1.96x     | 1.96 ± 0.12     |
| warm-cache `.' '/home/alexc' -HI`                                      | 122.9 ± 1.0     | 220.5 ± 3.4     | 1.79x     | 1.79 ± 0.03     |
| warm-cache `.' '/tmp/llvm-project' -HI`                                | 18.2 ± 0.7      | 36.0 ± 1.4      | 1.98x     | 1.98 ± 0.10     |
| warm-cache `.' '..' -HI`                                               | 18.7 ± 0.7      | 38.2 ± 1.8      | 2.04x     | 2.04 ± 0.12     |
| warm-cache `-HI '.*[0-9].*(md\|\.c)$' '/home/alexc`                    | 111.9 ± 1.4     | 178.4 ± 1.1     | 1.59x     | 1.59 ± 0.02     |
| warm-cache `-HI '.*[0-9].*(md\|\.c)$' '/tmp/llvm-project`              | 15.7 ± 0.5      | 29.4 ± 0.8      | 1.87x     | 1.87 ± 0.08     |
| warm-cache `-HI --size +1mb '' '/home/alexc`                           | 318.8 ± 10.7    | 674.4 ± 2.6     | 2.12x     | 2.12 ± 0.07     |
| warm-cache `-HI --size '+1mb' '' '/tmp/llvm-project`                   | 51.0 ± 3.1      | 139.7 ± 2.5     | 2.74x     | 2.74 ± 0.17     |
| warm-cache `-HI --size -1mb '' '/home/alexc`                           | 800.4 ± 15.2    | 1707.8 ± 19.1   | 2.13x     | 2.13 ± 0.05     |
| warm-cache `.' '/home/alexc' -HI --type d`                             | 210.0 ± 2.3     | 438.6 ± 16.1    | 2.09x     | 2.09 ± 0.08     |
| warm-cache `.' '/tmp/llvm-project' -HI --type d`                       | 15.2 ± 0.5      | 31.1 ± 1.1      | 2.05x     | 2.05 ± 0.10     |
| warm-cache `.' '/home/alexc' -HI --type e`                             | 885.5 ± 11.1    | 1202.9 ± 5.0    | 1.36x     | 1.36 ± 0.02     |
| warm-cache `.' '/tmp/llvm-project' -HI --type e`                       | 49.2 ± 2.0      | 105.6 ± 3.7     | 2.15x     | 2.15 ± 0.12     |
| warm-cache `.' '/home/alexc' -HI --type x`                             | 649.2 ± 6.1     | 869.4 ± 4.1     | 1.34x     | 1.34 ± 0.01     |
| warm-cache `.' '/tmp/llvm-project' -HI --type x`                       | 39.3 ± 2.2      | 54.4 ± 1.2      | 1.38x     | 1.38 ± 0.08     |
| warm-cache-ignore `-H --extension 'c' '' '/home/alexc'`                | 293.0 ± 20.7    | 604.6 ± 19.1    | 2.06x     | 2.06 ± 0.16     |
| warm-cache-ignore `.' '/home/alexc' -H`                                | 739.8 ± 183.8   | 1.453 ± 0.045   | 1.96x     | 1.96 ± 0.49     |
| warm-cache-ignore `.' '/tmp/llvm-project' -H`                          | 137.6 ± 6.5     | 230.4 ± 10.1    | 1.67x     | 1.67 ± 0.11     |
| warm-cache-ignore `-H '.*[0-9].*(md\|\.c)$' '/home/alexc'`             | 696.4 ± 47.3    | 1.305 ± 0.057   | 1.87x     | 1.87 ± 0.15     |
| warm-cache-ignore `-H --size +1mb '' '/home/alexc'`                    | 1.244 ± 0.076   | 2.505 ± 0.135   | 2.01x     | 2.01 ± 0.16     |
| warm-cache-ignore `.' '/tmp/llvm-project' -H --type e`                 | 230.1 ± 10.4    | 324.0 ± 24.3    | 1.41x     | 1.41 ± 0.12     |

```

--*Average Speedup:  2.11x*--

## Distinctions from fd/find

Symlink resolution in my method differs from fd and find. Although I generally advise against following symlinks, the option exists for completeness.

When following symlinks, behaviour will vary slightly. For example, fd can enter infinite loops with recursive symlinks
 (see recursive_symlink_fs_test.sh) [Available here](./scripts/recursive_symlink_fs_test.sh)
whereas my implementation prevents hangs. It may, however, return more results than expected.

To avoid issues, use --same-file-system when traversing symlinks. This ensures traversal terminates safely even in complex directories such as ~/.steam, ~/.wine, /sys, and /proc.

## Technical Highlights

### Key Optimisations

- **getdents64/getdents: Optimised the Linux/Android-specific/OpenBSD/NetBSD/Illumos/Solaris directory reading by significantly reducing the number of stat/statx/fstatat system calls**

- **Reverse engineered MacOS syscalls(`__getdirentries64`) to exploit early EOF and no unnecessary stat/pthread_mutex calls at [link here](./src/fs/iter.rs#L581)
 (Also works on FreeBSD)**

- **memrchr optimisation with 20%~ improvement on stdlib (SWAR optimisation)**

- **An optimised gitignore parser with 5x fewer stat64/statx calls**.

- **A custom written crossbeam workstealing parallel traversal algorithm**

### Constant-Time Directory Entry Processing

The following function provides an elegant solution to avoid branch mispredictions/SIMD instructions during directory entry parsing (a performance-critical loop):

Check source code for further explanation [in utils.rs](./src/util/utils.rs#L195)**

(This version is simplified from the actual implementation)

```rust
// Computational complexity: O(1) - truly constant time
// Used mostly on Linux type systems
// SIMD within a register, so no architecture dependence
//http://www.icodeguru.com/Embedded/Hacker%27s-Delight/043.htm
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    use core::mem::offset_of;
    use core::num::NonZeroU64;
    /*The only unsafe action is dereferencing the pointer; This MUST be validated beforehand */
    const LO_U64: u64 = u64::from_ne_bytes([0x01; size_of::<u64>()]);
    const HI_U64: u64 = u64::from_ne_bytes([0x80; size_of::<u64>()]);
    // Create a mask for the first 3 bytes in the case where reclen==24
    const MASK: u64 = u64::from_ne_bytes([0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]);
    const DIRENT_HEADER_START: usize = offset_of!(dirent64, d_name);
    let reclen = unsafe { (*drnt).d_reclen as usize };
    // Access the last 8 bytes of the word (this is an aligned read due to kernel providing 8 byte aligned dirent structs!)
    let mut last_word: u64 = unsafe { drnt.byte_add(reclen - 8).cast::<u64>().read() };
    // reclen is always multiple of 8 so alignment is guaranteed
    let mask = MASK * ((reclen == 24) as u64); // branchless mask (multiply by 0 or 1)
    last_word |= mask; //Mask out the false nulls when d_name is short (when reclen==24)
    //The idea is to convert each 0-byte to 0x80, and each nonzero byte to 0x00
    #[cfg(target_endian = "little")]
    //SAFETY: The u64 can never be all 0's post-mask
    let masked_word =
        unsafe { NonZeroU64::new_unchecked(last_word.wrapping_sub(LO_U64) & !last_word & HI_U64) };
    //http://0x80.pl/notesen/2016-11-28-simd-strfind.html#algorithm-1-generic-simd
    // ^ Reference for the BE algorithm
    // Use a borrow free algorithm to do this on BE safely(1 more instruction than LE)
    #[cfg(target_endian = "big")]
    //SAFETY: The u64 can never be all 0's post-mask
    let masked_word = unsafe {
        NonZeroU64::new_unchecked(
            (!last_word & !HI_U64).wrapping_add(LO_U64) & (!last_word & HI_U64),
        )
    };
    // Find the position of the null terminator
    #[cfg(target_endian = "little")]
    let byte_pos = (masked_word.trailing_zeros() >> 3) as usize;
    #[cfg(target_endian = "big")]
    let byte_pos = (masked_word.leading_zeros() >> 3) as usize;
    // reclen-DIRENT_HEADER start is the maximum size of the string
    // we then use the position of the `true` null terminator and subtract the 8, it's junk.
    reclen - DIRENT_HEADER_START + byte_pos - 8
}

```

## Why?

I started this project because I found find slow and wanted to learn how to interface directly with the kernel.
What began as a small experiment became a practical tool for exploring low-level systems work.

### Performance Motivation

Rust's std::fs has inefficiencies for this workload, including extra allocation, file descriptor handling overhead, repeated strlen work, and readdir-based traversal. Rewriting those paths with libc allowed tighter control over traversal costs and was a useful learning exercise.

The standard library can also keep file descriptors open until the last reference to an inner `ReadDir` disappears, which can become limiting on Unix systems with lower descriptor limits.

It also tends to rely heavily on `stat`-style calls, which is costly in traversal-heavy workloads.

See [fd_benchmarks/syscalltest.sh](./fd_benchmarks/syscalltest.sh) for a rough syscall comparison.

### Development Philosophy

**Feature stability before breakage - I won't push breaking changes or advertise this anywhere until I've got a good baseline.**

**Open to contributions - Once the codebase stabilises, contributions are welcome.**

In short, this project explores performance, low-level programming, and practical tooling.

## Acknowledgements/Disclaimers

I've directly taken code from [fnmatch-regex, found at the link](https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574) and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for some SWAR tricks from the standard library [(see link)](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
I additionally emailed the author of memchr and got some nice tips, great guy, someone I respect whole heartedly!

## Future Plans

### Feature Enhancements (Planned)

**API cleanup, currently the CLI is the main focus but I'd like to fix that eventually!**

**POSIX Compliance**: Mostly done, I don't expect to extend this beyond Linux/BSD/MacOS/Illumos/Solaris/Android (the other ones are embedded mostly, correct me if i'm wrong!), I have tentative work for other OS'es, it may support NuttX/few others but completely untested.

Ultimately, these are an extremely fringe usecase and I think it is beyond pointless to focus on these.

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

# Null terminated all output instead of newlines, mainly for command passing to other functions
fdf -HI --print 0 . ~ | xargs -0 realpath




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

  -S, --sort
          Sort the entries alphabetically (this has quite the performance cost)

  -s, --case-sensitive
          Enable case-sensitive matching, defaults to false

  -e, --extension <EXTENSION>
          An example command would be `fdf -HI -e  c '^str' /

  -j, --threads <THREAD_NUM>
          Number of threads to use, defaults to available threads available on your computer

  -a, --absolute-path
          Starts with the directory entered being resolved to full

  -L, --follow
          Include symlinks in traversal,defaults to false

      --nocolour
          Disable colouring output when sending to terminal

  -g, --glob
          Use a glob pattern,defaults to off

  -n, --max-results <TOP_N>
          Retrieves the first eg 10 results, 'fdf  -n 10 '.cache' /

  -d, --depth <DEPTH>
          Retrieves only traverse to x depth

  -p, --full-path
          Use a full path for regex matching, default to false

  -F, --fixed-strings
          Use a fixed string not a regex, defaults to false

      --show-errors
          Show errors when traversing

      --same-file-system
          Only traverse the same filesystem as the starting directory

  -0, --print0
          Makes all output null terminated as opposed to newline terminated only applies to non-coloured output and redirected(useful for xargs)

  -I, --no-ignore
          Do not respect .gitignore rules during traversal

      --strip-cwd-prefix
          Strip the leading './' from results when searching the current directory

  -Q, --quoted
          Wrap printed file paths in double quotes

      --exec <CMD>...
          Execute a command once per search result. Use '{}' to insert the matched path into an argument; if '{}' is omitted, the path is appended as the final argument. This option should be the final CLI flag
                  Example: 'fdf 'junk.files' 'test_directory' -HI --exec rm -rf ' , delete all files meeting the criteria

      --ignore <PATTERN>
          Ignore paths that match this regex pattern (repeatable)

      --ignoreg <GLOB>
          Ignore paths that match this glob pattern (repeatable)

      --ignore-file <path>
          Add a custom ignore-file in '.gitignore' format. These files have a low precedence.

      --and <pattern>
          Add additional required search patterns, all of which must be matched. Multiple additional patterns can be specified. The patterns are regular expressions, unless '--glob' or '--fixed-strings' is used.

      --size <SIZE>
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

          Possible values:
          - 100:   exactly 100 bytes
          - 1k:    exactly 1 kilobyte (1000 bytes)
          - 1ki:   exactly 1 kibibyte (1024 bytes)
          - 10mb:  exactly 10 megabytes
          - 1gb:   exactly 1 gigabyte
          - +1m:   larger than 1MB
          - +10mb: larger than 10MB
          - +1gib: larger than 1GiB
          - -500k: smaller than 500KB
          - -10mb: smaller than 10MB
          - -1gib: smaller than 1GiB

  -T, --time-modified <TIME>
          Filter by file modification time

          PREFIXES:
            -TIME    Find files modified within the last TIME (newer)
            +TIME    Find files modified more than TIME ago (older)
             TIME    Same as -TIME (default)

          TIME RANGE:
            TIME..TIME   Find files modified between two times

          UNITS:
            s, sec, second, seconds     - Seconds
            m, min, minute, minutes     - Minutes
            h, hour, hours              - Hours
            d, day, days                - Days
            w, week, weeks              - Weeks
            y, year, years              - Years

          EXAMPLES:
            --time -1h        Files modified within the last hour
            --time +2d        Files modified more than 2 days ago
            --time 1d..2h     Files modified between 1 day and 2 hours ago
            --time -30m       Files modified within the last 30 minutes

          Possible values:
          - -1h:    modified within the last hour
          - -30m:   modified within the last 30 minutes
          - -1d:    modified within the last day
          - +2d:    modified more than 2 days ago
          - +1w:    modified more than 1 week ago
          - 1d..2h: modified between 1 day and 2 hours ago

  -t, --type <TYPE_OF>
          Filter by file type

          Possible values:
          - d: Directory
          - u: Unknown type
          - l: Symbolic link
          - f: Regular file
          - p: Pipe/FIFO
          - c: Character device
          - b: Block device
          - s: Socket
          - e: Empty file
          - x: Executable file

      --generate <GENERATE>

              Generate shell completions for bash/zsh/fish/powershell/elvish
              To use: eval "$(fdf --generate SHELL)"
              Example:
              # Add to shell config for permanent use
              echo 'eval "$(fdf --generate zsh)"' >> ~/.zshrc && source ~/.zshrc

          [possible values: bash, elvish, fish, powershell, zsh]

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
  - Linux-only feature, making it a low-priority and high-effort addition.  **I will likely NOT do this**

#### 2. Allocation-Optimised Iterator Adaptor

- Implement a filtering mechanism that avoids unnecessary directory allocations.
- Achieved via a closure-based approach triggered during `readdir` or `getdents` calls.
- Although the cost of allocations doesn't seem too bad, I will look at this again at some point.
- Maybe achieved via a lending iterator type approach? See [link for reference](https://docs.rs/lending-iterator/latest/lending_iterator/)
