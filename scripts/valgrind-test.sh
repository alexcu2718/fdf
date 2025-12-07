#!/usr/bin/env bash


cd "$(dirname "$0" )" || exit
cd ..

# avoid optimisations using intrinsics(valgrind fucks up with avx2)
RUSTFLAGS="-C opt-level=0" cargo build --release

 valgrind --tool=memcheck --leak-check=full --show-leak-kinds=all \
          --trace-children=yes --log-file=valgrind.log \
          ./target/release/fdf 'NOMATCHLOL' / -HI


cat valgrind.log
