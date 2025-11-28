#!/usr/bin/env bash

set -e

cd "$(dirname "$0" )"
cd ..


cargo b -r


PATTERN="."
FDF_COMMAND="./target/release/fdf -HI $PATTERN $HOME"
FD_COMMAND="fd -HI $PATTERN $HOME"



perf stat -e cache-misses,cache-references,L1-dcache-load-misses,L1-dcache-loads,LLC-load-misses,LLC-loads,branches,branch-misses $FDF_COMMAND > /dev/null

perf stat -e cache-misses,cache-references,L1-dcache-load-misses,L1-dcache-loads,LLC-load-misses,LLC-loads,branches,branch-misses $FD_COMMAND > /dev/null
