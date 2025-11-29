#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

run_warm_benchmark "size-test" "-HI --size +1mb '' '$SEARCH_ROOT'" "-HI --size +1mb '' '$SEARCH_ROOT'" "size"
