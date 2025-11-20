 #!/usr/bin/env bash



### This is an experimental script for running benchmarks on different buffer sizes
### IT TAKES A WHILE
cd "$(dirname "$0" )"


cd ../fd_benchmarks

TABLE_SCRIPT="../scripts/make_results_table.sh"


TMP_DIR="${TMP:-/tmp}"

LLVM="$TMP_DIR/llvm-project"

if ! which hyperfine > /dev/null 2>&1; then
    echo "'hyperfine' does not seem to be installed."
    echo "You can get it here: https://github.com/sharkdp/hyperfine?tab=readme-ov-file#installation"
    exit 1
fi


if [[ -d "$LLVM" ]]; then
   :
else
    echo "$LLVM LLVM not found in location!"
    exit 1
fi




# Function to run benchmarks for a specific buffer size
run_buffer_size_test() {
    local buffer_size=$1
    echo "Testing buffer size: $buffer_size"
    
    export BUFFER_SIZE=$buffer_size
    cargo b -r
    
    for script in ./warm*.sh; do
        echo "Running $script with buffer size $buffer_size"
        ./"$script"
        sleep 2
    done
    SEND_TO="../scripts/${buffer_size}_buffer_size.out"
    $TABLE_SCRIPT > $SEND_TO
    echo "Results saved to $SEND_TO "
    rm -rf ../bench_results/*
    rm ../results_table.md
}

# loop over different buffer sizes (increments of 2000 starting from 4096)
for i in {0..10}; do
    buffer_size=$((4096 + i * 2000))
    run_buffer_size_test $buffer_size
done


