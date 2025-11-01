#!/usr/bin/env bash



TMP_DIR="${TMP:-/tmp}"

llvm_link=https://github.com/llvm/llvm-project
LLVM="$TMP_DIR/llvm-project"

fdf_location="$TMP_DIR/fdf_test"

fdf_repo=https://github.com/alexcu2718/fdf

SEARCH_ROOT="$LLVM"



alias sort='sort --parallel=$(nproc)' #speed up sorting speed



if [ ! -e "$LLVM" ] && [ -e "$HOME/llvm-project" ]; then
    echo "Found llvm-project in HOME directory, copying to $LLVM" #internal convenience trick for me, since cloning llvm is a pain in the ass.
    cp -r "$HOME/llvm-project" "$TMP_DIR/"
elif [ ! -e "$LLVM" ]; then
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
