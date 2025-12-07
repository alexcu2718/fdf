#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"

EXTENSION="c"

echo "running extension test"
run_warm_benchmark "file-extension" "-HI --extension '$EXTENSION' '' '$SEARCH_ROOT'" "-HI --extension '$EXTENSION' '' '$SEARCH_ROOT'" "extension"
