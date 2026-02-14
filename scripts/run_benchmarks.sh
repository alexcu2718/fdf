#!/usr/bin/env bash

cd "$(dirname "$0" )" || exit
cd ..

exec ./fd_benchmarks/run_all_tests_USE_ME.sh
