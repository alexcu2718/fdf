#!/bin/bash
cd "$(dirname "$0" )"
# Execute all warm*.sh scripts in the current directory


for script in ./warm*.sh; do
  echo "running $script"   
 ./"$script"
 echo "sleeping for 2 seconds"
 sleep 2
done

##quick hack to delete it in case people complain 
if [[ -d /tmp/llvm-project ]]; then
    read -p "/tmp/llvm-project exists. Delete it? [y/N]: " delete_confirm
    if [[ "$delete_confirm" =~ ^[Yy]$ ]]; then
        rm -rf /tmp/llvm-project
        echo "Deleted /tmp/llvm-project."
    else
        echo "Keeping /tmp/llvm-project."
    fi
fi



cargo test


read -p "Do you want to run benchmarks for 3 strlen implementations? [y/N]: " confirm

if [[ "$confirm" =~ ^[Yy]$ ]]; then

    cargo bench

else
  echo "Skipping benchmarks."
fi
