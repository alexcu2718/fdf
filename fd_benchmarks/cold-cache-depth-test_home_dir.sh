#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

DEPTH_LIMIT=4
run_cold_benchmark "depth-test_home_dir" "'.' '$HOME' -HI -d $DEPTH_LIMIT" "'.' '$HOME' -HI -d $DEPTH_LIMIT" "depth_home_dir"
