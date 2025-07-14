# fdf - High-Performance POSIX File Finder

 **Experimental** tool for regex/glob matching with colorized output.
Not production-ready: API unstable, renaming pending. Use with caution.

**i do have benchmark suites!**

## INTRO

NOT IN A STATE FOR USE/CONTRIBUTION, YE HAVE BEEN WARNED!

I have to change the name first and make the API actually coherent (I haven't tried using it as a crate yet)

As I fix and improve certain features, I will make it open to contributions.

Honestly this is still a hobby project that still needs much work.

It works for the subset I've implemented perfectly but it's far from complete.
It has better performance than `fd` on equivalent featuresets but `fd`
has an immense set, of which I'm not going to replicate
Rather that I'm just working on this project for myself because I really wanted to know what
happens when you optimally write hardware specific code( and how to write it!)

## How to test
```bash
git clone <https://github.com/alexcu2718/fdf>  /tmp/fdf_test &&   /tmp/fdf_test/fd_benchmarks/run_all_tests_USE_ME.sh
```

BE WARNED, I CLONE THE LLVM REPO TO CREATE A SUSTAINABLE ENVIRONMENT FOR TESTING, I DO THIS SPECIFICALLY IN /tmp
so this will be deleted at next shutdown, same goes for macos,
*not BSD (well, I only played around in QEMU, seems they've got a distinctively different system)*

IT DOES provide the option to delete this though

This runs a **comprehensive** suite of internal library+CLI tests as well as benchmarks.

## Cool bits(full benchmarks can be seen in speed_benchmarks.txt)

Testing on my local filesystem (to show on non-toy example)

```bash
| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf .  '/home/alexc' -HI --type l` | 259.2 ± 5.0 | 252.7 | 267.5 | 1.00 | #search my whole pc
| `fd -HI '' '/home/alexc' --type l` | 418.2 ± 12.8 | 402.2 | 442.6 | 1.61 ± 0.06 |


| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
| `fdf -HI --extension 'jpg' '' '/home/alexc'` | 292.6 ± 2.0 | 289.5 | 295.8 | 1.00 |
| `fd -HI --extension 'jpg' '' '/home/alexc'` | 516.3 ± 5.8 | 509.1 | 524.1 | 1.76 ± 0.02 |

######
#some of my benchmarks. repeatable on your own pc (as above)
#
# REPEATABLE BENCHMARKS (FOUND IN THE *HOW TO TEST* section above)
fd count: 12445
fdf count: 12445


##### searching for the extension c in the llvm repo 

Benchmark 1: fdf -HI --extension 'c' '' '/tmp/llvm-project'
  Time (mean ± σ):      20.6 ms ±   2.9 ms    [User: 39.3 ms, System: 119.3 ms]
  Range (min … max):    15.5 ms …  25.1 ms    12 runs

Benchmark 2: fd -HI --extension 'c' '' '/tmp/llvm-project'
  Time (mean ± σ):      35.1 ms ±   2.7 ms    [User: 141.0 ms, System: 108.9 ms]
  Range (min … max):    31.7 ms …  42.5 ms    12 runs

Summary
  fdf -HI --extension 'c' '' '/tmp/llvm-project' ran
    1.71 ± 0.27 times faster than fd -HI --extension 'c' '' '/tmp/llvm-project'
############
running ./warm-cache-type-simple-pattern.sh  
 fd count: 174329
fdf count: 174329

Running benchmarks...
Benchmark 1: fdf -HI '.*[0-9].*(md|\.c)$' '/tmp/llvm-project'
  Time (mean ± σ):      23.3 ms ±   3.2 ms    [User: 55.5 ms, System: 122.4 ms]
  Range (min … max):    16.4 ms …  28.3 ms    12 runs
 
Benchmark 2: fd -HI '.*[0-9].*(md|\.c)$' '/tmp/llvm-project'
  Time (mean ± σ):      34.2 ms ±   3.4 ms    [User: 124.2 ms, System: 105.4 ms]
  Range (min … max):    29.6 ms …  41.9 ms    12 runs
Summary
  fdf -HI '.*[0-9].*(md|\.c)$' '/tmp/llvm-project' ran
    1.47 ± 0.25 times faster than fd -HI '.*[0-9].*(md|\.c)$' '/tmp/llvm-project'

Summary
  fdf '.' '/tmp/llvm-project' -HI ran
    1.50 ± 0.21 times faster than fd '.' '/tmp/llvm-project' -HI
#####
running ./warm-cache-type-filtering-executable.sh   ######## TYPE FILTERING (REQUIRES LSTAT CALLS)
fd count: 927
fdf count: 927

Running benchmarks...
Benchmark 1: fdf '.' '/tmp/llvm-project' -HI --type x
  Time (mean ± σ):      33.2 ms ±   2.7 ms    [User: 49.6 ms, System: 225.1 ms]
  Range (min … max):    29.5 ms …  38.0 ms    12 runs

Benchmark 2: fd '.' '/tmp/llvm-project' -HI --type x
  Time (mean ± σ):      48.7 ms ±   1.6 ms    [User: 159.5 ms, System: 233.2 ms]
  Range (min … max):    46.6 ms …  51.2 ms    11 runs

Summary
  fdf '.' '/tmp/llvm-project' -HI --type x ran
    1.47 ± 0.13 times faster than fd '.' '/tmp/llvm-project' -HI --type x

```

## Extra bits

-dirent_const_strlen
 a  constant function which gets strlen from a dirent64 in constant time with no branches, only applicable to Linux

-cstr! :a macro  use a byte slice as a pointer (automatically initialise memory, then add **null terminator** for FFI use)

```rust

use fdf::cstr;
let who_is_that_pointer_over_there:*const u8=unsafe{cstr!("i'm too cheeky aren't i")};
//automatically  create an inline null stack allocated of length PATH_MAX(4096) and add a null terminator
let leave_me_alone:*const u8=unsafe{cstr!("hello_mate",5)}; //this will CRASH because you've only told to stack allocate for 5
/*explosions*/
//hence why its unsafe!
let this_is_fine_though:*const u8= unsafe{cstr!("hellohellohellohello",100)};


```

-A black magic macro that can colour filepaths based on a compile time perfect hashmap
(I made it into a separate crate)
it's defined in another github repo of mine at <https://github.com/alexcu2718/compile_time_ls_colours>

Then this function, really nice way to avoid branch misses during dirent parsing (a really hot loop)

```Rust



//The code is explained better in the true function definition (this is crate agnostic)
//This is the little-endian implementation, see crate for modified version for big-endian
// Only used on Linux systems, OpenBSD/macos systems store the name length trivially.
use fdf::find_zero_byte_u64; // a const SWAR function
pub const unsafe fn dirent_const_time_strlen(dirent: *const libc::dirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(libc::dirent64, d_name) + 1;
    let reclen = unsafe { (*dirent).d_reclen as usize }; 
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; //endianness fix omitted for brevity.
    let mask = 0x00FF_FFFFu64 * ((reclen ==24) as u64); //no branch
    let candidate_pos = last_word | mask;//^
    let byte_pos = 7 -  find_zero_byte_u64(candidate_pos) ; // a constant time SWAR function

    reclen - DIRENT_HEADER_START - byte_pos
}

```

## WHY?

Well, I found find slow, I didn't know fd existed, I didn't expect some random test project to actually be good.

Then finally, the reward is a tool I can use for the rest of my life to find stuff.

Mostly though, I just enjoy learning.

To put it in perspective, I did not know any C before I started this project, I noticed that every type of file finding tool will inevitably rely on some kind of iterator that will heap allocate regardless of whether or not it's a match, we're talking a hella lot of random allocations which I suspect may be a big bottleneck.

Even though my project in it's current state is faster, I've got some experiments to try filtering before allocating
Unfortunately, you have to have to allocate heap space for directories in stdlib (because they're necessary for the next call)

Which is partially why I felt the need to rewrite it from libc, it's just the standard library was too high level.

I'm curious to see what happens when you filter before allocation, this is something I have partially working in my current crate
but the implementation details like that is not accessible via CLI. If it proves to be performant, it will eventually be in there.
Obviously, I'm having to learn a lot to do these things and it takes  TIME to understand, get inspired and implement things...

I do intend to only add features and not break anything, until i can somewhat promise that, then i won't entertain wasting other people's time
with my nonsense so if anyone felt like adding something, you can!

(notably, there's some obvious things I have not touched in a while and things that are just less interesting, ideally one day someone could do that, not now though)

## NECESSARY DISCLAIMERS (I might have a conscience somewhere)

I've directly taken code from <https://docs.rs/fnmatch-regex/latest/src/fnmatch_regex/glob.rs.html#3-574> and modified it so I could convert globs to regex patterns trivially, this simplifies the string filtering model by delegating it to rust's extremely fast regex crate.
Notably I modified it because it's quite old and has a lot of silly dependencies (i removed all of them).

I've also done so for here <https://doc.rust-lang.org/src/core/slice/memchr.rs.html#111-161>
I've found a much more rigorous way of doing some bit tricks via this, there's unstable features included so I thought I'd appreciate the backing
of validated work like stdlib to ideally 'covalidate' my work, aka less leaps of logic required to make the assessment.

Some bits MIGHT be broken, the main use case to any user is error free
(unfortunately with FFI, they'll just pop up more later on, I have too many tests and it's really just that easy to get them in C)

## Future plans?

Separation of utilities

Right now, it's a bit monolithic. Some aspects might deserve their own crate (i dislike the idea of having 500 crates to do 1 specific thing each)

I'd probably just keep the CLI stuff simple, features to be added are datetime based filtering (could be done quick, I just have rarely used time based filtering and that's why it's slow!) as well as just other things, eg to search for device drivers/etc.

Add POSIX compatibility in general ( illumos/solaris QEMU isn't straight forward, quite esoteric)

Add Windows... Well, This would take a fundamental rewrite because of architectural differences, I might do it.
(It may be interesting to learn the differences actually)

Additional features on my compile_time_ls_colours would be nice, I think I want to explore compile time hashmaps more,
I'm only really scratching the service on metaprogramming and rust's utilities are great (one day I'll try cpp template metaprogramming and become a convert...)

Fundamentally I want to develop something that's simple to use (doing --help shouldnt give you the bible)
..and exceedingly efficient.

## COMPATIBILITY STATE

1.Working on Linux(MUSL too) 64bit                                             Tested on Debian/Ubuntu/Arch/Fedora varying versions

2.Somehow working on Aarch 64 Linux/Android Debian (basically, it works on my phone via termux!) (( and I didn't need to change anything!))

2.Macos  64bit  (Tested on Sonoma)

3.Free/Open/Net/Dragonfly BSD 64bit                             (Ok, it compiles on these platforms but only tested on freebsd+openbsd.)

4.Works on big endian systems, tested on PPC64 (took 20 minutes to compile....)

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
  [PATH]     Path to search (defaults to current working directory )


Options:
  -E, --extension <EXTENSION>  filters based on extension, eg -E .txt or -E txt

  -H, --hidden                 Shows hidden files eg .gitignore or .bashrc, defaults to off

  -s, --case-sensitive         Enable case-sensitive matching, defaults to false

  -j, --threads <THREAD_NUM>   Number of threads to use, defaults to available threads
                                [default: NUM_CORES]
  -a, --absolute-path          Show absolute paths of results, defaults to false

  -I, --include-dirs           Include directories, defaults to off

  -L, --follow                 Include symlinks in traversal,defaults to false

  -g, --glob                   Use a glob pattern,defaults to off

  -n, --max-results <TOP_N>    Retrieves the first N results (not ordered!)

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
   This will be extremely challenging. Unfortunately uring lacks the op code required for getdents, however
   other op codes are available, but this would require a LOT of work, it also would require an async runtime
   Which inevitably means tokio, which means most of my work in avoiding dependencies goes down the bin
   (I'm already unhappy being reliant on rayon but that's on the list to remove.)

-- String Interning: Trivial for ASCII, but efficient Unicode handling is an entirely different beast.
   (although, creating an enum at compile time of common filepaths on your pc and doing some manipulations sounds cool+cursed)

-- I might continue developing my compile time hashmap for LS_COLORS, it's got a good general use case and the macro use is pretty fun!
   However I do have a separate commit at <https://github.com/alexcu2718/compile_time_ls_colours/tree/no_phf_build>
   Which has no dependencies, although it's REALLY shit to do without doing a HELLA lot of byte manipulation yourself.
   (also, it's runtime statically initialised, not as cool!)

-- Threading Without Rayon: My attempts have come close, but aren’t quite there yet.
   I'll rely on Rayon for now until I can come up with a smart way to implement an appropriate work-distributing algorithm. TODO!

-- Iterator Adaptor + Filter: Some kind of adaptor that avoids a lot of unnecessary allocations on non-directories.

-- Syscall Limits: I think there’s ultimately a hard limit on syscalls.
   I've experimented with an early Zig `io_uring + getdents` implementation — but it's well outside my comfort zone (A LOT).
   I’ll probably give it a go anyway (if possible).

**** THIS IS NOT FINISHED. I have no idea what the long-term plans are — I'm just trying to make stuff go fast and learn, OK?
