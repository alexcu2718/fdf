#!/usr/bin/env bash

##this is just a simple struct for inspecting all the dirent structs in libc
## naturally, libc is located in my home folder, I didn't want to include it in this repo.

cd "$(dirname $0)"

[ -d ~/libc ] || { echo "~/libc not found"; exit 1; }



rg 'pub struct dirent' -n ~/libc | cut -d: -f1 | sort -u | while read -r file; do
  echo "### $file"
  awk '/pub struct dirent/ {found=1} found {print} /\}/ && found {found=0}' "$file"
  echo
done > dirent_structs.txt

cat dirent_structs.txt
