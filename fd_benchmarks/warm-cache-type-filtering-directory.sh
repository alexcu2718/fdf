#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"


run_warm_benchmark "type-filtering-directory" "'.' '$SEARCH_ROOT' -HI --type d" "'.' '$SEARCH_ROOT' -HI --type d" "type_d"
