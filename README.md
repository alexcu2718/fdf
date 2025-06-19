# fdf

**Name to be changed, I just entered this randomly on my keyboard, it sounds like fd-faster which is funny but thats not my intent,hence name change

Probably the fastest finder you'll find on Linux for regex/glob matching files (see benchmark proof versus fd*)

*For reference <https://github.com/sharkdp/fd>

Honestly this is still a hobby project that still needs much work.
It's functional, etc.

NOT IN A STATE FOR USE/CONTRIBUTION, YE HAVE BEEN WARNED!

The CLI is basically an afterthought because I'm focusing on lower levels and going up in functionality, like ascending Plato's cave (increasing abstraction)

It has better performance than `fd` on equivalent featuresets but 'fd'
has an immense set, of which I'm not going to replicate
Rather that I'm just working on this project for myself because I really wanted to know what happens when you optimally write hardware specific code( and how to write it!)

I'd probably just keep the CLI stuff simple and add some additional stuff like metadata/etc filtering.

Fundamentally I want to develop something that's simple to use (doing --help shouldnt give you the bible)...and
exceedingly efficient.

The caveat is you have to have contextual information from a dirent64, so it only works for file system operations, a cool trick nonetheless!

Cool bits:

Speed! In every benchmark so far tested, it's ranging from 1.1-2x as fast for regex/glob feature sets, check the benchmark!

cstr! macro: use a byte slice as a pointer (automatically initialise memory, add null terminator for FFI use) or alternatively cstr_n (MEANT FOR FILEPATHS!)

dirent_const_strlen const fn, get strlen from a dirent64 in constant time with no branches (benchmarks below)

BytePath: Cool deref trait for working with &[u8]


## SHORTTSTRINGS(~8)

(PLEASE NOT I HAVE TRIMMED AWAY THE UNNECESSARY INFO FROM THESE TO RETAIN MOST PERTINENT INFORMATION
SEE BENCHMARKS IN const_str_benchmark.txt for better details and ideally read my benches/dirent_bench.rs)

```bash

 strlen_by_length/const_time_swar/empty
                        time:   [877.68 ps 879.00 ps 880.32 ps]
                        thrpt:  [0.0000   B/s 0.0000   B/s 0.0000   B/s]
strlen_by_length/libc_strlen/empty
                        time:   [1.4977 ns 1.4994 ns 1.5012 ns]
                        thrpt:  [0.0000   B/s 0.0000   B/s 0.0000   B/s
```

## LONGSTRINGS(~240)

```bash
 strlen_by_length/const_time_swar/xlarge (129-255)
                       time:   [1.0687 ns 1.0719 ns 1.0750 ns]
strlen_by_length/libc_strlen/xlarge (129-255)
                        time:   [3.1976 ns 3.2114 ns 3.2255 ns]
                        thrpt:  [57.747 GiB/s 58.000 GiB/s 58.252 GiB/s]
```

```Rust
//The code is explained better in the true function definition (this is crate agnostic)
//This is the little-endian implementation, see crate for modified version for big-endian
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1; 
    let reclen = unsafe { (*dirent).d_reclen as usize }; //(do not access it via byte_offset!)
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) };
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch
    let candidate_pos = last_word | mask;//^
    let zero_bit = candidate_pos.wrapping_sub(0x0101_0101_0101_0101)
        & !candidate_pos //no branch, see comments for hack
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

```bash
| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf .  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 |
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |


| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 | 
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |
```

TODO LIST MAYBE:
--Arena allocator potentially,  written from scratch (see microsoft's edit for a nice one) //github.com/microsoft/edit/tree/main/src/arena

--io_uring for Batched Syscalls: E.g., batched open/read ops. This will be extremely challenging.

--String Interning: Trivial for ASCII, but efficient Unicode handling is another beast entirely.

--Threading without rayon: My attempts have come close but aren’t quite there yet. I'll rely on rayon for now until I can think of a smart way to implement an appropriate work distributing algorithm, TODO!

--Some sort of iterator adaptor+filter, which would allow one to avoid a lot more allocations on non-directories.

--I think there's ultimately a hard limit in syscalls, I've played around with an experimental zig iouring getdents implementation but it's out of my comfort zone, A LOT, I'll probably do it still(if possible)

****THIS IS NOT FINISHED, I have no idea what the plans are, i'm just making stuff go fast and learning ok.

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
