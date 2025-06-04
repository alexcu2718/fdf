
# fdf - Fast Directory Finder(LINUX ONLY!!!!!!!!!!!!! )

This project began as a way to deepen my understanding of Rust, particularly
its low-level capabilities and performance optimisations.
By building a filesystem traversal tool from scratch, I aimed to explore syscalls, memory safety,
and parallelism—while challenging myself to match or exceed the speed of established tools like fd.

I only picked up linux about half a year ago so I wanted to do something involving C/Rust/Linux/Assembly(if required) because
there's no point in making some cookie cutter TODO project. Go hard or go home.

Philosophical aspect:
One could argue that despite this crate having some merits, was it WORTH it?
Yes, because almost everything here will be reused in some concept/form/etc, you learn a tool properly, you don't need to create quantity,
just quality.

*nix Compatibility::::::::(I'm not sure, openbsd may be easier to write, i'm pretty sure the same syscall works but i think)
Tested on EXT4/BTRFS on Debian/Ubuntu/Arch, no issues.
NO CLUE on BSD-MAY WORK (might just do some experiments on a VM)
MacOS



TODO LIST MAYBE:
BUMP ALLOCATOR potentially, potentially written from scratch (see microsoft's edit for a nice bump)


****THIS IS NOT FINISHED, THIS WILL BE ABOUT 2025/06-07 for semi-comparable featureset with fd.

**A high-performance file search utility for Linux systems**, designed to quickly traverse and filter your filesystem.

---

## Features

- **Ultra-fast multi-threaded directory traversal**
- **Powerful regex pattern matching** (with glob support via `-g`)
- **Extension filtering** (`-E jpg,png`)
- **Hidden file toggle** (default: excluded)
- **Case sensitivity control** (`-s` for case-sensitive)
- **File type filtering** (files, directories via `-t`)
- **Thread configuration** for performance tuning (`-j 8`)
- **Max results limit** (`-d 100`)
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

  -j, --threads <THREAD_NUM>   Number of threads to use, defaults to available threads [default: X]
  -a, --absolute-path          Show absolute path
  -I, --include-dirs           Include directories

  -g, --glob                   Use a glob pattern
  -n, --max-results <TOP_N>    Retrieves the first eg 10 results, rlib rs$ -d 10
  -d, --depth <DEPTH>          Retrieves only traverse to x depth
      --generate <GENERATE>    Generate shell completions [possible values: bash, elvish, fish, powershell, zsh]
  -t, --type <TYPE_OF>...      Select type of files (can use multiple times)
  -p, --full-path              Use a full path for regex matching
  -F, --fixed-strings          Use a fixed string not a regex
  -h, --help                   Print help
  -V, --version                Print version
  