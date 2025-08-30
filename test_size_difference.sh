#!/bin/bash
echo "this script is an example of a disparity in size filtering i'm seeing, quite nebulous"
echo "fd output count is  $(cat fd.out | wc -l ) "
( fdf . /tmp/llvm-project -HI --size -500b ) | sort > fdf.out
echo "fdf output count is $(cat fdf.out | wc -l ) "
echo "there is a disparity of 1 file in fdf that isnt in fd"
diff fdf.out fd.out | grep '^<' | cut -d' ' -f2-
