#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"
ask_for_sudo

echo "running size filtering test for $HOME  , these can take up to 2mins in worst case due to benchmarking multiple runs"
run_cold_benchmark "size-test_negative_home_dir" "-HI --size -1mb '' '$HOME'" "-HI --size -1mb '' '$HOME'" "size_home_negative"
