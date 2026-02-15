# fdf - High-Performance POSIX File Finder

[![Rust CI](https://github.com/alexcu2718/fdf/workflows/Rust/badge.svg)](https://github.com/alexcu2718/fdf/actions)

fdf is a high-performance POSIX file finder written in Rust with extensive C FFI.

It serves as a lightweight alternative to tools such as fd and find, with a focus on speed, efficiency, and cross-platform compatibility. Benchmarks demonstrate fdf running up to 2x faster than comparable tools, achieved through low-level optimisation, SIMD techniques, and direct kernel interfacing.

PLEASE NOTE: This is due to undergo a rename before a 1.0

**Quick Installation:**

```bash
cargo install --git https://github.com/alexcu2718/fdf

## Additionally specify  --no-default-features to remove mimalloc dependency
```

## Project Status

This is primarily a learning and performance exploration project. Whilst already useful and performant, it remains under active development towards a stable 1.0 release. The name 'fdf' is a temporary placeholder.

The implemented subset performs exceptionally well, surpassing fd in equivalent feature sets, though fd offers broader functionality. This project focuses on exploring hardware-specific code optimisation rather than replicating fd's complete feature set.

While the CLI is usable, the internal library is not stable yet. Alas!

## Platform Support (64-bit only (32 bit maybe one day?))

### Fully Supported and CI Tested

- Linux (x86_64, s390x (Big endian), Alpine( MUSL libc))
- macOS (Intel and Apple Silicon)
- FreeBSD (x86_64)
- Solaris(x86_64)

### Compiles with Limited Testing

*Note: GitHub Actions does not yet provide Rust 2024 support for some(most of these) platforms. Additional checks will be added when available.*

- OpenBSD (Specifically tested recently on a VM)
- NetBSD, DragonflyBSD (tested occasionally, minor fixes expected if issues arise, tested on QEMU occasionally)
- Android (tested on my phone)
- Illumos (Solaris works, illumos is essentially identical, I'll test it eventually)

- I have removed aarch64 Linux and riscv Linux from Github actions due to *VERY UNRELIABLE RUNNERS*

### Not Yet Supported

- **Windows**: Requires significant rewrite due to architectural differences with libc. Planned once the POSIX feature set is stable. Windows already has highly effective tools such as [Everything](https://www.voidtools.com/). The plan is this to work on this after a 1.0.

### Non supported filesystems

This tool doesn't support reiserfs in any form, due to it's extremely long filename length, every other file system is supported, it's not worth sacrificing
the performance improvements to support an extremely niche fs that is used by 0.001% of people(if that...).

It's deliberately got a build script to stop building on reiser.

## Testing

The project includes comprehensive testing with 90+ Rust tests and 15+ correctness benchmarks comparing against fd.

Note: Miri validation (Rust's undefined behaviour detector) cannot be used due to the extensive libc calls. Intensive testing and valgrind validation are used instead. See the [valgrind script here](./scripts/valgrind-test.sh)

- Rust tests: [Available here](./src/test.rs)
- Shell scripts clone the LLVM repository to provide an accurate testing environment
- Tests run via GitHub Actions on all supported platforms

**Running the Full Test Suite:**

```bash

TMP_DIR="${TMP:-/tmp}"
git clone --depth 1 https://github.com/alexcu2718/fdf "$TMP_DIR/fdf_test"
cd "$TMP_DIR/fdf_test"


# If on Android, ensure the script is executable
if [[ "$(uname -o)" == "Android" ]]; then
    chmod +x ./scripts/run_benchmarks.sh
fi

./scripts/run_benchmarks.sh
```

This executes a comprehensive suite of internal library tests, CLI tests, and benchmarks.

## Performance Benchmarks

The benchmarks are fully repeatable using the testing code above and cover file type filtering, extension matching, file sizes, and many other scenarios. The following results were obtained on a local system and the LLVM repo to provide realistic usage  examples:
(These are tests done via hyperfine and summarised to save space here.)

(*TESTED ON LINUX, other OS's will (probably) be lower due to specific linux optimisations)

(I cannot test accurately on qemu due to virtualisation overhead and I do not have a mac)

Rough tests indicate a significant 75%+ speedup on BSD's/Illumos/Solaris but macos has less optimisations, macos is a much heftier OS and I struggle to emulate it stably (I get bizarre results from SSHing into it and running dtruss)

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

```

--*Average Speedup:  2.16x*--

## Distinctions from fd/find

Symlink resolution in my method differs from fd and find. Although I generally advise against following symlinks, the option exists for completeness.

When following symlinks, behaviour will vary slightly. For example, fd can enter infinite loops with recursive symlinks
 (see recursive_symlink_fs_test.sh) [Available here](./scripts/recursive_symlink_fs_test.sh)
whereas my implementation prevents hangs. It may, however, return more results than expected.

To avoid issues, use --same-file-system when traversing symlinks. Both fd and find also handle them poorly without such flags. My approach ensures the program always terminates safely, even in complex directories like ~/.steam, ~/.wine, /sys, and /proc.

The flag -I includes directories in output(as opposed to ignore files), I will change this in future.

## Technical Highlights

### Key Optimisations

- **getdents64/getdents: Optimised the Linux/Android-specific/OpenBSD directory reading by significantly reducing the number of stat/statx/fstatat system calls**

- **Reverse engineered MacOS syscalls(`__getdirentries64`) to exploit early EOF and no unnecessary stat calls at [link here](./src/fs/iter.rs#L581)**

-**Also has an optimised path for `FreeBSD` using `getdirentries`**

- **memrchr optimisation with 20%~ improvement on stdlib (SWAR optimisation)**

- **Compile-time colour mapping**: A compile-time perfect hashmap for colouring file paths, defined in a [separate repository](https://github.com/alexcu2718/compile_time_ls_colours)

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
 #[cfg(any(target_os = "linux",target_os = "android",target_os = "emscripten",
        target_os = "redox", target_os = "hermit", target_os = "fuchsia"))]
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    use core::num::NonZeroU64;
    use core::mem::offset_of;
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
    //SAFETY: The u64 can never be all 0's post-SWAR
    let zero_bit = unsafe {
            NonZeroU64::new_unchecked(last_word.wrapping_sub(LO_U64) & !last_word & HI_U64)
        };
    //http://0x80.pl/notesen/2016-11-28-simd-strfind.html#algorithm-1-generic-simd
    // ^ Reference for the BE algorithm
    // Use a borrow free algorithm to do this on BE safely(1 more instruction than LE)
    #[cfg(target_endian = "big")]
    //SAFETY: The u64 can never be all 0's post-SWAR
    let zero_bit = unsafe {
            NonZeroU64::new_unchecked(
                (!last_word & !HI_U64).wrapping_add(LO_U64) & (!last_word & HI_U64),
            )
        };

    // Find the position of the null terminator
    #[cfg(target_endian = "little")]
    let byte_pos = (zero_bit.trailing_zeros() >> 3) as usize;
    #[cfg(target_endian = "big")]
    let byte_pos = (zero_bit.leading_zeros() >> 3) as usize;
    // reclen-DIRENT_HEADER start is the maximum size of the string
    // we then use the position of the `true` null terminator and subtract the 8, it's junk.
    reclen - DIRENT_HEADER_START + byte_pos - 8
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

Notably the standard library will keep file descriptors open(UNIX specific) until the last reference to the inner `ReadDir` disappears, because UNIX has a limit on open file descriptors, this can cause a form of 'rate limiting', not ideal.

It will also tend to call 'stat' style calls heavily which is very! inefficient

(I do have a shell script documenting syscall differences here(it's crude but it works well)) [Available here](./fd_benchmarks/syscalltest.sh)

### Development Philosophy

** Feature stability before breakage - I won't push breaking changes or advertise this anywhere until I've got a good baseline.

** Open to contributions - Once the codebase stabilises, I welcome others to add features if they're extremely inclined anyway!

In short, this project is a personal exploration into performance, low-level programming, and building practical tools - with the side benefit of making a useful tool and learning a crazy amount!

## Acknowledgements/Disclaimers

I've directly taken code from [fnmatch-regex, found at the link](https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574) and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has dependencies I was able to remove

(I have emailed and received approval from the author above)

I've also done so for some SWAR tricks from the standard library [(see link)](https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161)
I additionally emailed the author of memchr and got some nice tips, great guy, someone I respect whole heartedly!

## Future Plans

### Feature Enhancements (Planned)

More elaborate improvements/fixes discussed [at this link]( ./IMPROVEMENTS.md   )

**API cleanup, currently the CLI is the main focus but I'd like to fix that eventually!**

**POSIX Compliance**: Mostly done, I don't expect to extend this beyond Linux/BSD/MacOS/Illumos/Solaris/Android (the other ones are embedded mostly, correct me if i'm wrong!), I have tentative work for other OS'es, but ultimately it is hard to even emulate these! Such as l4re,horizon etc.
Some OS'es are plainly not supported, such as vita/nuttx (due to lacking inodes) and hurd (due to unbounded filenames)

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

  -I, --include-dirs
          Include directories, defaults to off

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

  -0, --print0
          Makes all output null terminated as opposed to newline terminated, only applies to non-coloured output and redirected(useful for xargs)

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

  -T, --time <TIME>
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
          Filter by file type:
            d, dir, directory    - Directory
            u, unknown           - Unknown type
            l, symlink, link     - Symbolic link
            f, file, regular     - Regular file
            p, pipe, fifo        - Pipe/FIFO
            c, char, chardev     - Character device
            b, block, blockdev   - Block device
            s, socket            - Socket
            e, empty             - Empty file
            x, exec, executable  - Executable file

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

#### 2. Optimisations for the BSD family

- Part done. Need to implement getdirentries/getdents for NetBSD/DragonFlyBSD (after this done, NO MORE platform specific code!)

#### 3. Allocation-Optimised Iterator Adaptor

- Implement a filtering mechanism that avoids unnecessary directory allocations.
- Achieved via a closure-based approach triggered during `readdir` or `getdents` calls.
- Although the cost of allocations doesn't seem too bad, I will look at this again at some point.
- Maybe achieved via a lending iterator type approach? See [link for reference](https://docs.rs/lending-iterator/latest/lending_iterator/)
