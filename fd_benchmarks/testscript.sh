#!/bin/bash

llvm_link=https://github.com/llvm/llvm-project
LLVM=/tmp/llvm-project

fdf_location=/tmp/fdf_test
pff_location=/tmp/pretty_fast_find

fdf_repo=https://github.com/alexcu2718/fdf

pff_repo=https://github.com/pericles-tpt/pretty_fast_find



# Clone llvm-project if not already present
if [ ! -e "$LLVM" ]; then
    echo "cloning llvm repo $llvm_link, this may take a while sorry!"
    git clone "$llvm_link" "$LLVM" > /dev/null 2>&1
else
    echo "$LLVM already found, not cloning repo"
fi




# Clone and build fdf if not already installed
if [ ! -e "$fdf_location" ]; then
    echo "Cloning fdf to $fdf_location..."
    git clone "$fdf_repo" "$fdf_location" > /dev/null
    cd "$fdf_location" || exit 1
    cargo build --release -q
else
    echo "fdf already installed at $fdf_location"
fi

# Clone and build pretty_fast_find if not already installed
if [ ! -e "$pff_location" ]; then
    echo "Cloning pretty_fast_find to $pff_location..."
    git clone "$pff_repo" "$pff_location" > /dev/null
    cd "$pff_location" || exit 1
    cargo build --release -q
else
    echo "pretty_fast_find already installed at $pff_location"
fi

# Export paths to built binaries
export PATH="$PATH:$fdf_location/target/release"
export PATH="$PATH:$pff_location/target/release"






search_counts() {
    local PATTERN="$1"
    echo "fdf result count for pattern $PATTERN: $(fdf "$PATTERN" -HI "$LLVM" | wc -l)"
    echo "fd result count for pattern $PATTERN:  $(fd "$PATTERN" -HI "$LLVM" | wc -l)"
    echo "pff result count for pattern $PATTERN: $(pff "$PATTERN" "$LLVM" | wc -l)"
}


pattern_1="."
pattern_2="\.c$"
pattern_3="arb"
search_counts $pattern_1
search_counts $pattern_2
search_counts $pattern_3


new_pattern='i$'
# Start benchmarking
hyperfine --warmup 1 \
  "fdf $new_pattern -HI  $LLVM "   \
   "fd $new_pattern -HI $LLVM "    \
  "pff  $new_pattern  $LLVM"  \


read -p "Do you wish    to delete $LLVM? $fdf_location and $pff_location [y/N] " confirm
if [[ "$confirm" == [yY] ]]; then
    rm -rf "$LLVM"
    rm -rf "$pff_location"
   rm  -rf "$fdf_location"
    echo "Directories have been deleted  been deleted."
else
    echo "Deletion cancelled."
fi

echo 'script doneso'
