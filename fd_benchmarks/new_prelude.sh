#!/usr/bin/env bash



if [[ "${BASH_SOURCE[0]}" != "${0}" ]]; then
    cd "$(dirname "${BASH_SOURCE[0]}")" || exit
else

    cd "$(dirname "$0")" || exit
fi



ask_for_sudo() {
    echo "This script will now ask for your password in order to gain root/sudo"
    echo "permissions. These are required to reset the harddisk caches in between"
    echo "benchmark runs."
    echo ""

    sudo echo "Okay, acquired superpowers :-)" || exit

    echo ""
}



export WARMUP_COUNT=5

# command for "cold cache" benchmarks
export RESET_CACHES="sync; echo 3 | sudo tee /proc/sys/vm/drop_caches"


TMP_DIR="${TMP:-/tmp}"

llvm_link=https://github.com/llvm/llvm-project
LLVM="$TMP_DIR/llvm-project"

fdf_location="$TMP_DIR/fdf_test"

fdf_repo=https://github.com/alexcu2718/fdf
# shellcheck disable=SC2034
SEARCH_ROOT="$LLVM"

EXCLUDE='paru/clone/.*/pkg|systemd-private|fd.*\.lst$'



if [ ! -e "$LLVM" ] && [ -e "$HOME/llvm-project" ]; then
    echo "Found llvm-project in HOME directory, copying to $LLVM" #internal convenience trick for me, since cloning llvm is a pain in the ass.
    cp -r "$HOME/llvm-project" "$TMP_DIR/"
elif [ ! -e "$LLVM" ]; then
    echo "cloning llvm repo $llvm_link, this may take a while sorry!"
    git clone --depth 1 "$llvm_link" "$LLVM" >/dev/null 2>&1
else
    :
fi


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

run_warm_benchmark() {
    local benchmark_name="$1"
    local fdf_args="$2"
    local fd_args="$3"
    local output_basename="${4:-$benchmark_name}"
    local skip_diff="${5:-0}"

    OUTPUT_DIR="./bench_results"
    mkdir -p "$OUTPUT_DIR"


    local COMMAND_FIND="fdf $fdf_args"
    local COMMAND_FD="fd $fd_args"


    if [[ "$skip_diff" == "0" ]]; then
        # (filter out paru and systemd temporary files (these are protected files so these would be false results anyway))
        echo -e "\nGetting accurate file counts..."
        fd_count=$(eval "$COMMAND_FD" | grep -vcE "$EXCLUDE" )
        fdf_count=$(eval "$COMMAND_FIND" | grep -vcE "$EXCLUDE")
        echo "fd count: $fd_count"
        echo "fdf count: $fdf_count"
    fi


    echo -e "\nRunning benchmarks..."
    hyperfine \
        --warmup "$WARMUP_COUNT" \
        --prepare 'sync; sleep 0.2' \
        "$COMMAND_FIND" \
        "$COMMAND_FD" \
        --export-markdown "$OUTPUT_DIR/results-warm-cache-${benchmark_name}.md"

    if [[ "$skip_diff" == "0" ]]; then
        # sorted output lists (filter out paru and systemd files)
        eval "$COMMAND_FD" | grep -vE "$EXCLUDE" | sort --parallel="$(nproc)" > "$OUTPUT_DIR/fd_${output_basename}.lst"
        eval "$COMMAND_FIND" | grep -vE "$EXCLUDE" | sort --parallel="$(nproc)" > "$OUTPUT_DIR/fdf_${output_basename}.lst"

        diff -u "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" > "$OUTPUT_DIR/fd_diff_${output_basename}.md"

        differences=$(comm -3 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | wc -l)
        echo "Total lines differing: $differences"

        if [[ $differences -gt 0 ]]; then
            echo -e "\nFiles only in fd (showing first 10):"
            echo -e "\n\n\n THESE FILES ARE USUALLY EXCLUDED DUE PERMISSIONS (fd usually being at /usr/bin/fd )"
            comm -23 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | head -n 10

            echo -e "\nFiles only in fdf (showing first 10):"
            comm -13 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | head -n 10

            echo -e "\nFull diff available in: $(realpath $OUTPUT_DIR/fd_diff_"${output_basename}".md)"
        else
            echo "No differences found in direct execution"
        fi

        # Report results locations
        echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-${benchmark_name}.md"
        echo "Diff results saved to $OUTPUT_DIR/fd_diff_${output_basename}.md"
    else
        echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-warm-cache-${benchmark_name}.md"
    fi
}


run_cold_benchmark() {
    local benchmark_name="$1"
    local fdf_args="$2"
    local fd_args="$3"
    local output_basename="${4:-$benchmark_name}"
    local min_runs="${5:-3}"

    OUTPUT_DIR="./bench_results"
    mkdir -p "$OUTPUT_DIR"

    local COMMAND_FIND="fdf $fdf_args"
    local COMMAND_FD="fd $fd_args"

    echo -e "\nRunning cold cache benchmarks..."
    hyperfine \
        --min-runs "$min_runs" \
        --prepare "$RESET_CACHES" \
        "$COMMAND_FIND" \
        "$COMMAND_FD" \
        --export-markdown "$OUTPUT_DIR/results-cold-cache-${benchmark_name}.md"



    eval "$COMMAND_FD" | grep -vE "$EXCLUDE" | sort --parallel="$(nproc)" > "$OUTPUT_DIR/fd_${output_basename}.lst"
    eval "$COMMAND_FIND" | grep -vE "$EXCLUDE" | sort --parallel="$(nproc)" > "$OUTPUT_DIR/fdf_${output_basename}.lst"

    diff -u "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" > "$OUTPUT_DIR/fd_diff_${output_basename}.md"


    differences=$(comm -3 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | wc -l)
    echo "Total lines differing: $differences"

    if [[ $differences -gt 0 ]]; then
        echo -e "\nFiles only in fd (showing first 10):"
        comm -23 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | head -n 10

        echo -e "\nFiles only in fdf (showing first 10):"
        comm -13 "$OUTPUT_DIR/fd_${output_basename}.lst" "$OUTPUT_DIR/fdf_${output_basename}.lst" | head -n 10

        echo -e "\nFull diff available in: $(realpath $OUTPUT_DIR/fd_diff_"${output_basename}".md)"
    else
        echo "No differences found in direct execution"
    fi


    echo -e "\nBenchmark results saved to $OUTPUT_DIR/results-cold-cache-${benchmark_name}.md"
    echo "Diff results saved to $OUTPUT_DIR/fd_diff_${output_basename}.md"
}
