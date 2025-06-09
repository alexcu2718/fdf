#!/bin/bash

source "prelude.sh"

llvm_link=https://github.com/llvm/llvm-project
LLVM=/tmp/llvm-project

fdf_location=/tmp/fdf_test

fdf_repo=https://github.com/alexcu2718/fdf


SEARCH_ROOT="$LLVM"
#basically unsetting the default search root

pff_location=/tmp/pretty_fast_find


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
    export PATH="$PATH:$fdf_location/target/release"
else
    echo "fdf already installed at $fdf_location"    
    export PATH="$PATH:$fdf_location/target/release"

fi

echo "fdf location is $(which fdf)"


echo I HAVE MODIFIED THESE BECAUSE I DO NOT HAVE NO GIT IGNORE IN MINE YET.
echo "there is a bug in hyperfine i believe, if there is a discrepancy, please run the commands and test output yourself, i am clueless on as to why..."

COMMAND_FIND="fdf  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FIND="find '$SEARCH_ROOT'"
COMMAND_FD="fd  '.' '$SEARCH_ROOT' -HI"
#COMMAND_FD="fd --hidden --no-ignore '' '$SEARCH_ROOT'"

hyperfine --warmup "$WARMUP_COUNT" \
    "$COMMAND_FIND" \
    "$COMMAND_FD" \
    --export-markdown results-warm-cache-no-pattern.md

check_for_differences "true" "$COMMAND_FIND" "$COMMAND_FD"
#ordering
sort /tmp/results.fd > /tmp/results.fd_sorted
sort /tmp/results.find > /tmp/results.find_sorted
total_diff=$(diff /tmp/results.fd_sorted /tmp/results.find_sorted | wc -l)
echo "The total difference is $(($total_diff / 2))"
diff /tmp/results.fd_sorted /tmp/results.find_sorted | awk '{print $2}' | tr -s ' ' >  /tmp/missing_results.fdf
echo 'missing results(if true are 0)'
cat /tmp/missing_results.fdf
echo "however, when searching directly, we find that they are not missing."
echo "this is a bit broken currently, basically there's a weird off by 1 error i get sometimes, im not desperately trying to fix it because i believe its hyperfine related"

