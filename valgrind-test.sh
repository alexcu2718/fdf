 valgrind --tool=memcheck --leak-check=full --show-leak-kinds=all \
          --trace-children=yes --log-file=valgrind.log \
          ./target/release/fdf 'NOMATCHLOL' / -HI

cat valgrind.log
