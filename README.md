# fdf

COMPATIBILITY STATE (BIG+LITTLE ENDIAN COMPATIBLE)

1.Working on Linux 64bit

2.Macos  64bit

3.Free/Open/Net BSD 64bit

3.Tested on 64bit PPC Linux (Ubuntu)

5.Alpine/MUSL

NOT IN A STATE FOR USE/CONTRIBUTION, YE HAVE BEEN WARNED!

**Name to be changed, I just entered this randomly on my keyboard, it sounds like fd-faster which is funny but thats not my intent,hence name change

Probably the fastest finder you'll find on Linux for regex/glob matching files (see benchmark proof versus fd*)

Honestly this is still a hobby project that still needs much work.
It's functional, etc.

The CLI is basically an afterthought because I'm focusing on lower levels and going up in functionality, like ascending Plato's cave (increasing abstraction)

It has better performance than `fd` on equivalent featuresets but 'fd'
has an immense set, of which I'm not going to replicate
Rather that I'm just working on this project for myself because I really wanted to know what happens when you optimally write hardware specific code( and how to write it!)

## Future plans?

I'd probably just keep the CLI stuff simple

Add some extra metadata filters (because i get a lot of metadata for cheap via specialisation!)

Add POSIX compatibility in general (not too bad) (BSD completed!)

Add Windows...(maybe?) .

Too many internal changes.

Fundamentally I want to develop something that's simple to use (doing --help shouldnt give you the bible)
..and exceedingly efficient.

## Cool bits

Speed! In every benchmark so far tested, it's ranging from a minimum of 1.2x and a maximum of 2x as fast~~ (really approximating here) as fast for regex/glob feature sets, check the benchmark!

dirent_const_strlen const fn, get strlen from a dirent64 in constant time with no branches (benchmarks below)

cstr! macro: use a byte slice as a pointer (automatically initialise memory, add null terminator for FFI use) or alternatively cstr_n (MEANT FOR FILEPATHS!)

BytePath: Cool deref trait for working with filepaths (derefs to &[u8])

This is a compile-time hash map of file extensions to their corresponding ANSI color codes based on the LS_COLORS environment variable.
defined as

```rust
pub static LS_COLOURS_HASHMAP: Map<&'static [u8], &'static [u8]>
```

(it's defined in another github repo of mine at <https://github.com/alexcu2718/compile_time_ls_colours>)

## SHORTSTRINGS(under 8 chars)

SEE BENCHMARKS IN const_str_benchmark.txt for better details and ideally read my benches/dirent_bench.rs

```bash

strlen_by_length/const_time_swar/tiny (1-4)
                           time:   [961.66 ps 964.31 ps 966.95 ps]
                         thrpt:  [986.27 MiB/s 988.97 MiB/s 991.69 MiB/s]
strlen_by_length/libc_strlen/tiny (1-4)
                          time:   [1.6422 ns 1.6466 ns 1.6511 ns]
                           thrpt:  [577.60 MiB/s 579.17 MiB/s 580.73 MiB/s]
 strlen_by_length/asm_strlen/tiny (1-4)
                          time:   [718.41 ps 720.59 ps 722.76 ps]
                          thrpt:  [1.2886 GiB/s 1.2925 GiB/s 1.2964 GiB/s]
```

## MAXLENGTHSTRINGS (255)

```bash
   strlen_by_length/const_time_swar/max length (255)
                         time:   [963.74 ps 966.35 ps 969.00 ps]
                         thrpt:  [245.09 GiB/s 245.76 GiB/s 246.42 GiB/s]
  strlen_by_length/libc_strlen/max length (255) #interesting!
                         time:   [3.3193 ns 3.3281 ns 3.3368 ns]
                        thrpt:  [71.172 GiB/s 71.359 GiB/s 71.548 GiB/s]
  strlen_by_length/asm_strlen/max length (255)
                        time:   [4.6074 ns 4.6290 ns 4.6513 ns]
                       thrpt:  [51.058 GiB/s 51.304 GiB/s 51.544 GiB/s]


```

```Rust
//The code is explained better in the true function definition (this is crate agnostic)
//This is the little-endian implementation, see crate for modified version for big-endian
// Only used on Linux systems, OpenBSD/macos systems store the name length trivially.
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1; 
    let reclen = unsafe { (*dirent).d_reclen as usize }; //(do not access it via byte_offset or raw const!!!!!!!!!!!)
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) };
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch
    let candidate_pos = last_word | mask;//^
    let zero_bit = candidate_pos.wrapping_sub(0x0101_0101_0101_0101)
        & !candidate_pos //no branch, see comments for hack
        & 0x8080_8080_8080_8080; 

    reclen - DIRENT_HEADER_START - (7 - (zero_bit.trailing_zeros() >> 3) as usize)
}


```bash
| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf .  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 |
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |


| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 | 
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |
```

## Requirements

- **Linux/Macos/Bsd only**: Specific posix syscalls.
- **64 bit tested only(+PPC BE64bit)**

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

 fdf --help
Usage: fdf [OPTIONS] [PATTERN] [PATH]

Arguments:
  [PATTERN]  Pattern to search for
  [PATH]     Path to search (defaults to /)
             Use -c to do current directory


Usage: fdf [OPTIONS] [PATTERN] [PATH]

Arguments:
  [PATTERN]  Pattern to search for
  [PATH]     Path to search (defaults to current working directory )


Options:
  -E, --extension <EXTENSION>  filters based on extension, eg -E .txt or -E txt

  -H, --hidden                 Shows hidden files eg .gitignore or .bashrc, defaults to off

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

-- String Interning: Trivial for ASCII, but efficient Unicode handling is an entirely different beast.

-- Threading Without Rayon: My attempts have come close, but aren’t quite there yet.
   I'll rely on Rayon for now until I can come up with a smart way to implement an appropriate work-distributing algorithm. TODO!

-- Iterator Adaptor + Filter: Some kind of adaptor that avoids a lot of unnecessary allocations on non-directories.

-- Syscall Limits: I think there’s ultimately a hard limit on syscalls.
   I've experimented with an early Zig `io_uring + getdents` implementation — but it's well outside my comfort zone (A LOT).
   I’ll probably give it a go anyway (if possible).

**** THIS IS NOT FINISHED. I have no idea what the long-term plans are — I'm just trying to make stuff go fast and learn, OK?
