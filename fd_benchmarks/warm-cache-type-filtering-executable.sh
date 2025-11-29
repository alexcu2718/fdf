#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

run_warm_benchmark "type-filtering-executable" "'.' '$SEARCH_ROOT' -HI --type x" "'.' '$SEARCH_ROOT' -HI --type x" "type_x"
