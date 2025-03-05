# fdf - Fast Directory Finder

## LINUX ONLY!!!!!!!!!!!!! (should work on openbsd?)

**A high-performance file search utility for Linux systems**, designed to quickly traverse and filter your filesystem.

## PLEASE NOTE

THIS PROJECT IS MEANT TO BE USED AS A COMMIT TO
FD AND IS ***NOWHERE*** NEAR COMPLETION.

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

- **Linux only**: Optimized for Linux filesystems
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

## Options

-c, --current-directory   Uses the current directory instead of the default path
-E, --extension <EXT>     Filters results by file extension
-H, --hidden              Shows hidden files (those starting with .)
-s, --case-sensitive      Enables case-sensitive pattern matching
-j, --threads <NUM>       Sets the number of threads to use (default: system available)
-I, --include-dirs        Includes directories in results
-g, --glob                Treats the pattern as a glob pattern
-d, --max-depth <NUM>     Limits results to first N matches
-t, --type <TYPE>...      Filters by file type (can be used multiple times)
-p, --full-path           Matches against full file paths rather than just names
-F, --fixed-strings       Treats pattern as a fixed string, not a regex
-h, --help                Displays help information
-V, --version             Displays version information
