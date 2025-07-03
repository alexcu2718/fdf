#!/bin/bash
cd "$(dirname "$0" )"
# Execute all warm*.sh scripts in the current directory


for script in ./warm*.sh; do
  echo "running $script"   
 ./"$script"
 echo "sleeping for 2 seconds"
 sleep 2
done


cargo test

read -p "Do you want to run benchmarks for 3 strlen implementations? [y/N]: " confirm

if [[ "$confirm" =~ ^[Yy]$ ]]; then

    cargo bench

else
  echo "Skipping benchmarks."
fi
