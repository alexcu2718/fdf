
# fdf - Fast Directory Finder(LINUX ONLY)

This project began as a way to deepen my understanding of Rust, particularly
its low-level capabilities and performance optimisations.
By building a filesystem traversal tool from scratch, I aimed to explore syscalls, memory safety,
and parallelism—while challenging myself to match or exceed the speed of established tools like fd(the fastest directory traversal tool with regex/glob others parameters)

(*For reference fd: https://github.com/sharkdp/fd (it has 19000* more stars than this repo!))

There's a lot of extremely niche optimisations hidden within this crate, also things I have never seen elsewhere!

My Crown jewel however is my constant(conditionally) time calculation for strlen with bo branches/simd required, all via bittricks+pointer manipulation :)

The caveat is you have to have contextual information froma dirent64, so it only works for file system operations, a cool trick nonetheless!

If you run cargo bench, it is constant(nearly) and MUCH faster than glibc strlen!

## SHORTTSTRINGS(~8)
```bash
strlen_comparison/dirent_const_time_single/case_1

                        time:   [1.0157 ns 1.0208 ns 1.0260 ns]
                        thrpt:  [8.7716 Gelem/s 8.8162 Gelem/s 8.8611 Gelem/s]
                 change:
                        time:   [−24.104% −23.281% −22.455%] (p = 0.00 < 0.05)
                        thrpt:  [+28.957% +30.346% +31.759%]
                        Performance has improved.
Found 12 outliers among 500 measurements (2.40%)
  10 (2.00%) high mild
  2 (0.40%) high severe
strlen_comparison/libc_strlen_single/case_1
                        time:   [2.2868 ns 2.3168 ns 2.3488 ns]
                        thrpt:  [3.8318 Gelem/s 3.8846 Gelem/s 3.9356 Gelem/s]
                 change:
                        time:   [+2.6941% +3.4345% +4.2209%] (p = 0.00 < 0.05)
                        thrpt:  [−4.0499% −3.3205% −2.6234%]
                        Performance has regressed.

## LONGESTSTRINGS(240~)

strlen_comparison/dirent_const_time_single/case_8
                        time:   [1.0524 ns 1.0571 ns 1.0618 ns]
                        thrpt:  [8.4765 Gelem/s 8.5142 Gelem/s 8.5523 Gelem/s]
                 change:
                        time:   [−47.073% −46.039% −45.017%] (p = 0.00 < 0.05)
                        thrpt:  [+81.875% +85.319% +88.938%]
                        Performance has improved.
Found 2 outliers among 500 measurements (0.40%)
  1 (0.20%) high mild
  1 (0.20%) high severe
strlen_comparison/libc_strlen_single/case_8
                        time:   [4.1926 ns 4.2135 ns 4.2376 ns]
                        thrpt:  [2.1238 Gelem/s 2.1360 Gelem/s 2.1466 Gelem/s]
                 change:
                        time:   [−33.151% −32.598% −32.052%] (p = 0.00 < 0.05)
                        thrpt:  [+47.170% +48.363% +49.592%]
```

```Rust
//The code is explained better in comments, it's 
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_SIZE: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) };
    let mask = 0x00FF_FFFFu64 * ((reclen / 8 == 3) as u64); 
    let zero_bit = (last_word | mask).wrapping_sub(0x0101_0101_0101_0101)// 
        & !(last_word | mask) 
        & 0x8080_8080_8080_8080; 
  
    reclen - DIRENT_HEADER_SIZE - (7 - (zero_bit.trailing_zeros() >> 3) as usize)
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

Feature set match: regex/glob/type filtering/extension matching, i've got to implement some gitignore stuff together, that's going to be very enjoyable lol.
but actually I have some ideas on how to do to this. I've got internal features for lots of things, but this doesn't extend to the CLI.

Please check the fd_benchmarks for more(run them yourself, please!)

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf .  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 |
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |

| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 |  
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |

Regarding the above: FD and FDF determine extensions differently. My implementation searches for .jpg (case-insensitively), whereas fd performs a case-insensitive regex match on jpg$. I contend that my approach is superior!

Motivation

I began using Linux around mid-2024, and from the outset, I wanted to avoid cookie-cutter projects like yet another TODO app. Instead, I aimed to dive into something challenging, I'm also great at losing stuff, how can I find it faster? Well this stupid 40k star library wont do, its time to get raw silicon out.
Joking aside, mostly, it's actually the thought I could make a tool that's generally useful.

Philosophical aspect:
One might question whether this effort was justified. Absolutely. Nearly every component here will be reused in some form or another. The goal here 
is primary that of my own learning! if I write an arena allocator here,do you think that knowledge gets deleted when I'm not on this project?

Compatibility Notes:
    Tested on: EXT4/BTRFS (Debian/Ubuntu/Arch) — no issues.
    BSD/macOS: Untested (might work; OpenBSD/FreeBSD could even offer performance benefits due to d_namelen in dirent, eliminating some strlen calls). I’ll need to experiment in a VM.
    Side note: Are OpenBSD and FreeBSD meaningfully distinct here? (Someone enlighten me!)

TODO LIST MAYBE:
--Arena allocator potentially,  written from scratch (see microsoft's edit for a nice one) //github.com/microsoft/edit/tree/main/src/arena

--io_uring for Batched Syscalls: E.g., batched open/read ops. This will be extremely challenging.

--String Interning: Trivial for ASCII, but efficient Unicode handling is another beast entirely.

--Threading Without Rayon: My attempts have come close but aren’t quite there yet. I can get within 10% speed wise but........thats not acceptable.

--Some sort of iterator adaptor+filter, which would allow one to avoid a lot more allocations on non-directories.

--I think there's ultimately a hard limit in syscalls, I've played around with an experimental zig iouring getdents implementation but it's out of my comfort zone, A LOT, I'll probably do it still(if possible)

****THIS IS NOT FINISHED, THIS WILL BE ABOUT 2025/06-07 for semi-comparable featureset with fd.

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

Usage: fdf [OPTIONS] [PATTERN] [PATH]

Arguments:
  [PATTERN]  Pattern to search for
  [PATH]     Path to search (defaults to /)
             Use -c to do current directory

Options:
  -c, --current-directory      Uses the current directory to load

  -E, --extension <EXTENSION>  filters based on extension, options are ['d', 'u', 'l', 'f', 'p', 'c', 'b', 's', 'e', 'x']

  -H, --hidden                 Shows hidden files eg .gitignore or .bashrc

  -s, --case-sensitive         Enable case-sensitive matching

  -j, --threads <THREAD_NUM>   Number of threads to use, defaults to available threads [default: 12]
  -a, --absolute-path          Show absolute path
  -I, --include-dirs           Include directories

  -g, --glob                   Use a glob pattern
  -n, --max-results <TOP_N>    Retrieves the first eg 10 results, '\.cache' / -n 10
  -d, --depth <DEPTH>          Retrieves only traverse to x depth
      --generate <GENERATE>    Generate shell completions [possible values: bash, elvish, fish, powershell, zsh]
  -t, --type <TYPE_OF>...      Select type of files (can use multiple times)
  -p, --full-path              Use a full path for regex matching
  -F, --fixed-strings          Use a fixed string not a regex, eg '.bashrc' / -FH 
  -h, --help                   Print help
  -V, --version                Print version
