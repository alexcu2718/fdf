#!/usr/bin/env bash

cd  "$(realpath "$(dirname "$0")")" || exit 1
# shellcheck disable=SC1091
source "new_prelude.sh"

SIZE="-1mb"
EXT="c"
PATTERN='^lib'
run_warm_benchmark "size-test" "-HI --type f --size '$SIZE' -e  '$EXT' '$PATTERN'  '$SEARCH_ROOT'" "-HI --type f --size '$SIZE' -e '$EXT' '$PATTERN' '$SEARCH_ROOT'" "size"
