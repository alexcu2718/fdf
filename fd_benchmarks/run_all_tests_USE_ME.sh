#!/usr/bin/env bash

cd "$(dirname "$0" )" || exit


TMP_DIR="${TMP:-/tmp}"

LLVM_DIR="$TMP_DIR/llvm-project"


if ! which hyperfine > /dev/null 2>&1; then
    echo "'hyperfine' does not seem to be installed."
    echo "You can get it here: https://github.com/sharkdp/hyperfine?tab=readme-ov-file#installation"
    exit 1
fi

# Check if llvm-project already exists
if [[ -d "$LLVM_DIR" ]]; then
    run_benchmarks="y"
else
    echo "This script will clone llvm-project into $LLVM_DIR for testing/validation purposes."
    read -rp "Do you want to run speed/correctness benchmarks for the llvm project? [y/n] " run_benchmarks
fi






if [[ "$run_benchmarks" =~ ^[Yy]$ ]]; then
    for script in ./warm*.sh; do
        if [[ "$script" != *"_home_dir"* ]]; then
            echo "Running $script"
            ./"$script"
            sleep 2
        fi
    done

    if [[ "$(uname -s)" == "Linux" ]]; then
        # Check if sudo exists(not available on android, well, it is, but i'm lazy and not rooting my phone!)
        if command -v sudo &> /dev/null; then
            echo "Running cold cache test..."
            ./cold-cache-simple-pattern.sh
        else
            echo "Skipping cold cache test because sudo is not available."
        fi
    else
        echo "Skipping cold cache test because it is only supported on Linux."
    fi
else
    echo "Skipping benchmarks."
fi




echo -e "\n\nTHERE WILL BE A SMALL DISPARITY IN THESE TESTS DUE TO fd being located in /usr/bin (USUALLY) ((different permissions!))\n, differences are expected to be very small";
echo -e "there will also be predictable temporary files created!\n\n"
echo "these tests will take a while!"
read -rp 'Do you want to run speed/correctness benchmarks for home directory? [y/n]:' run_benchmarks_home

if [[ "$run_benchmarks_home" =~ ^[Yy]$ ]]; then
    for script in ./warm*_home_dir.sh; do
        echo "Running $script"
        ./"$script"
        sleep 2
    done
else
    echo "Skipping benchmarks for home directory."
fi


if [[ "$(uname -s)" == "Linux" ]]; then
    read -rp "Do you want to run the syscall test? [y/n] " response

    if [[ "$response" =~ ^[Yy]$ ]]; then
        if command -v strace &> /dev/null; then
            echo "Running the syscall test..."
            ./syscalltest.sh
        else
            echo "Error: strace is not installed. Please install it to run this test."
        fi
    else
        echo "Skipping the syscall test."
    fi
else
    echo "Skipping syscall test because it is only supported on Linux."
fi

##quick hack to delete it in case people complain
if [[ -d "$LLVM_DIR" ]]; then
    read -rp "$LLVM_DIR exists. Delete it? [y/n]: " delete_confirm
    if [[ "$delete_confirm" =~ ^[Yy]$ ]]; then
        rm -rf "$LLVM_DIR"
        echo "Deleted $LLVM_DIR."
    else
        echo "Keeping $LLVM_DIR."
    fi
fi


read -rp "Do you want to run cargo test? [y/n]" response_test

if [[ "$response_test" =~ ^[Yy]$ ]]; then
    cargo test
else
    echo "Skipping cargo test"
fi



read -rp "Do you want to run benchmarks for 3 strlen implementations? [y/n]: " confirm

if [[ "$confirm" =~ ^[Yy]$ ]]; then

    cargo bench

else
  echo "Skipping benchmarks."
fi
