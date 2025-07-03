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

read -p "Do you want to run benchmarks for 3 strlen implementations(linux only for now)? [y/N]: " confirm

if [[ "$confirm" =~ ^[Yy]$ ]]; then
  if [[ "$(uname)" == "Linux" ]]; then
    echo "Detected Linux OS. Running cargo bench..."
    cargo bench
  else
    echo "Non-Linux OS detected. Skipping cargo bench."
  fi
else
  echo "Skipping benchmarks."
fi
