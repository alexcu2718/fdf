#!/usr/bin/env bash


llvm_link=https://github.com/llvm/llvm-project
LLVM=/tmp/llvm-project

fdf_location=/tmp/fdf_test

fdf_repo=https://github.com/alexcu2718/fdf
alias sort='sort --parallel=$(nproc)' #speed up sorting speed
SEARCH_ROOT="$LLVM"
#basically unsetting the default search root

# Clone llvm-project if not already present
if [ ! -e "$LLVM" ]; then
	echo "cloning llvm repo $llvm_link, this may take a while sorry!"
	git clone "$llvm_link" "$LLVM" >/dev/null 2>&1
else
	:
fi

# Clone and build fdf if not already installed
if [ ! -e "$fdf_location" ]; then
	echo "Cloning fdf to $fdf_location..."
	git clone "$fdf_repo" "$fdf_location" >/dev/null
	echo "Building fdf..."
	cd "$fdf_location" || exit 1
	cargo b -r 
else
	cd "$fdf_location" || exit 1
	cargo b -r -q #check if it's built just incase
 
fi

export PATH="$fdf_location/target/release:$PATH"
