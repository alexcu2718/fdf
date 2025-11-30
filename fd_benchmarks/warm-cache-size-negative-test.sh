#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

SIZE="-1mb"
run_warm_benchmark "size-test" "-HI --size '$SIZE' '' '$SEARCH_ROOT'" "-HI --size '$SIZE' '' '$SEARCH_ROOT'" "size"
