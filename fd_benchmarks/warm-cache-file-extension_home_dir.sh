#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"
EXT="c"
echo "running extension test"
run_warm_benchmark "file-extension_home_dir" "-HI --extension '$EXT' '' '$HOME'" "-HI --extension '$EXT' '' '$HOME'" "extension_home_dir"
