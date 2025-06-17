
# fdf - Fast Directory Finder(LINUX ONLY)

This project began as a way to deepen my understanding of Rust, particularly
its low-level capabilities and performance optimisations.
By building a filesystem traversal tool from scratch, I aimed to explore syscalls, memory safety,
and parallelism—while challenging myself to match or exceed the speed of established tools like fd(the fastest directory traversal tool with regex/glob others parameters)

Compatibility Notes:
    This is only tested in x86-64 systems, I'm 99% certain this wouldn't work on big-endian systems without some adaptations(mostly for u64 to u8 casts)
    Tested on: EXT4/BTRFS (Debian/Ubuntu/Arch) — no issues.
    BSD/macOS: 99% Chance of not working, in theory because this is mostly POSIX(d_dtype shortcut I use won't work, will need lstat)
    Side note: Are OpenBSD and FreeBSD meaningfully distinct here? (Someone enlighten me!)

(*For reference fd: <https://github.com/sharkdp/fd> (it has 19000* more stars than this repo and it's extremely fast!))

There's a lot of extremely niche optimisations hidden within this crate, things I have never seen elsewhere!

My Crown jewel however is my O(1)  calculation for strlen with bo branches/simd required, all via bit tricks+pointer manipulation :)

The caveat is you have to have contextual information froma dirent64, so it only works for file system operations, a cool trick nonetheless!

If you run cargo bench, it is constant(practically so, crytography nerds can look into it) and MUCH faster than glibc strlen!
(The reason this is such a good optimisation is it's one of the most called functions in this and both fd/find/etc, this is a unique advantage I could take by avoiding abstractions)

Note to anyone reading: I'm purposely not advertising this because It's not really in a state for contributors. Needs despaghettification at points. Also API is highly experimental(no nighly reliance yet, although I am tempted by some fancy stuff...)

## SHORTTSTRINGS(~8)

(PLEASE NOT I HAVE TRIMMED AWAY THE UNNECESSARY INFO FROM THESE TO RETAIN MOST PERTINENT INFORMATION
SEE BENCHMARKS IN const_str_benchmark.txt for better details and ideally read my benches/dirent_bench.rs)

```bash

 strlen_by_length/const_time_swar/empty
                        time:   [994.53 ps 997.11 ps 999.71 ps]
strlen_by_length/libc_strlen/empty
                          time:   [1.6360 ns 1.6408 ns 1.6455 ns]
```

## LONGSTRINGS(~240)

```bash
 strlen_by_length/const_time_swar/xlarge (129-255)
                       time:   [1.0687 ns 1.0719 ns 1.0750 ns]
 strlen_by_length/libc_strlen/xlarge (129-255)
                       time:   [4.2938 ns 4.3054 ns 4.3171 ns]
```

```Rust
//The code is explained better in the true function definition (this is crate agnostic)
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1; 
    let reclen = unsafe { (*dirent).d_reclen as usize }; //(do not access it via byte_offset!)
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) };
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); 
    let candidate_pos = last_word | mask;
    let zero_bit = candidate_pos.wrapping_sub(0x0101_0101_0101_0101)
        & !candidate_pos
        & 0x8080_8080_8080_8080; 

    reclen - DIRENT_HEADER_START - (7 - (zero_bit.trailing_zeros() >> 3) as usize)
}
```

instant build guide script for testing/the impatient:
(If you're on EXT4/BTRFS with a somewhat modern kernel, it'll work)



```bash

#!/bin/bash
dest_dir=$HOME/Downloads/fdf
mkdir -p $dest_dir
git clone https://github.com/alexcu2718/fdf $dest_dir
cd $dest_dir
cargo b -r -q 
export PATH="$PATH:$dest_dir/target/release"
echo "$(which fdf)"
```

And to my pleasure, I did succeed! Although, my featureset is dramatically lessened, somethings are a pain to implement,
It's also a hobby project I do when I'm bored, so I do things when I feel like them, why yes, how did you know I worked on Half-L....
There's also lots of performance benefits still to be gained.

Feature set found at bottom of post.

Please check the fd_benchmarks for more(run them yourself, please!)

As for some fairly arbitrary, first look benchmarks.

```bash
| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf .  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 |
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |



| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 | 
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |
```

Motivation

I began using Linux around mid-2024, and from the outset, I wanted to avoid cookie-cutter projects like yet another TODO app. Instead, I aimed to dive into something challenging, I'm also great at losing stuff, how can I find it faster? Well this stupid 40k star library wont do, its time to get raw silicon out.
Joking aside, mostly, it's actually the thought I could make a tool that's generally useful.

Philosophical aspect:
One might question whether this effort was justified. Absolutely. Nearly every component here will be reused in some form or another. The goal here 
is primary that of my own learning! if I write an arena allocator here,do you think that knowledge gets deleted when I'm not on this project?

TODO LIST MAYBE:
--Arena allocator potentially,  written from scratch (see microsoft's edit for a nice one) //github.com/microsoft/edit/tree/main/src/arena

--io_uring for Batched Syscalls: E.g., batched open/read ops. This will be extremely challenging.

--String Interning: Trivial for ASCII, but efficient Unicode handling is another beast entirely.

--Threading Without Rayon: My attempts have come close but aren’t quite there yet. I can get within 10% speed wise but........thats not acceptable.

--Some sort of iterator adaptor+filter, which would allow one to avoid a lot more allocations on non-directories.

--I think there's ultimately a hard limit in syscalls, I've played around with an experimental zig iouring getdents implementation but it's out of my comfort zone, A LOT, I'll probably do it still(if possible)

****THIS IS NOT FINISHED, Probably a long term project for for semi-comparable/full (or totally new one) featureset with fd.

---

## Features

- **Ultra-fast multi-threaded directory traversal**
- **Powerful regex pattern matching** (with glob support via `-g`)
- **Extension filtering** (`-E jpg,png`)
- **Hidden file toggle** (default: excluded)
- **Case sensitivity control** (`-s` for case-sensitive)
- **File type filtering** (files, directories via `-t`)
- **Thread configuration** for performance tuning (`-j 8`)
- **Max results limit** (`-n 100`)
- **Full path matching** (`-p`)
- **Fixed-string search** (non-regex via `-F`)

---

## Requirements

- **Linux only**: Specific linux syscalls for Linux filesystems
- **Rust 1.74+** (recommended for building from source)

---

## Installation

```bash
# Clone & build
git clone https://github.com/alexcu2718/fdf.git
cd fdf
cargo build --release

# Optional system install
cp target/release/fdf ~/.local/bin/

Usage
Arguments
PATTERN: Regular expression pattern to search for
PATH: Directory to search (defaults to root /)
Basic Examples
# Find all files containing "config" in the current directory and subdirectories (case-insensitive and excluding directories+hidden files)
fdf config -c

# Find all JPG files in the home directory (excluding hidden files)
fdf . ~ -E jpg

# Find all  Python files in /usr/local (including hidden files)
fdf . /usr/local -E py -H

## Options (T)

Options:
  -c, --current-directory      Uses the current directory to load

  -E, --extension <EXTENSION>  filters based on extension, eg -E .txt or -E txt
  -H, --hidden                 Shows hidden files eg .gitignore or .bashrc

  -s, --case-sensitive         Enable case-sensitive matching

  -j, --threads <THREAD_NUM>   Number of threads to use, defaults to available threads [default: 12]
  -a, --absolute-path          Show absolute path
  -I, --include-dirs           Include directories

  -g, --glob                   Use a glob pattern
  -n, --max-results <TOP_N>    Retrieves the first eg 10 results, '.cache' / -n 10
  -d, --depth <DEPTH>          Retrieves only traverse to x depth
      --generate <GENERATE>    Generate shell completions [possible values: bash, elvish, fish, powershell, zsh]
  -t, --type <TYPE_OF>...      Select type of files (can use multiple times), available options are:
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
  -p, --full-path              Use a full path for regex matching
  -F, --fixed-strings          Use a fixed string not a regex
  -h, --help                   Print help
  -V, --version                Print version

```
