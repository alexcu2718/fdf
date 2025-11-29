#!/usr/bin/env bash

# shellcheck disable=SC1091
source "new_prelude.sh"

REL_ROOT=".."


echo -e "\nRunning relative directory benchmarks..."
run_warm_benchmark "relative-dir-test" "'.' '$REL_ROOT' -HI" "'.' '$REL_ROOT' -HI" "relative"
