#!/bin/bash
cd "$(dirname "$0" )" || exit
# Execute all warm*.sh scripts in the current directory


read -rp "Do you want to run speed/correctness benchmarks? (y/n) " run_benchmarks
if [[ "$run_benchmarks" =~ ^[Yy]$ ]]; then
    for script in ./warm*.sh; do
        echo "Running $script"
        ./"$script"
        sleep 2
    done
else
    echo "Skipping benchmarks."
fi


##quick hack to delete it in case people complain 
if [[ -d /tmp/llvm-project ]]; then
    read -rp "/tmp/llvm-project exists. Delete it? [y/n]: " delete_confirm
    if [[ "$delete_confirm" =~ ^[Yy]$ ]]; then
        rm -rf /tmp/llvm-project
        echo "Deleted /tmp/llvm-project."
    else
        echo "Keeping /tmp/llvm-project."
    fi
fi


read -rp "Do you want to run the syscall test? (y/n) " response

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



read -rp "Do you want to run cargo test? (y/n) " response_test

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
