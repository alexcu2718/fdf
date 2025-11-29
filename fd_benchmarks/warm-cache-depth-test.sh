#!/usr/bin/env bash


# shellcheck disable=SC1091
source "new_prelude.sh"

DEPTH_LIMIT=2
echo -e "\nRunning depth-limited benchmarks (depth=$DEPTH_LIMIT)..."
run_warm_benchmark "depth-test" "'.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT" "'.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT" "depth"
