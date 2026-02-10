#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

DEPTH_LIMIT=2
echo -e "\nRunning depth-limited benchmarks (depth=$DEPTH_LIMIT)..."
run_cold_benchmark "depth-test" "'.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT" "'.' '$SEARCH_ROOT' -HI -d $DEPTH_LIMIT" "depth"
