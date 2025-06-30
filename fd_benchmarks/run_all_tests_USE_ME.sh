#!/bin/bash
cd "$(dirname "$0" )"
# Execute all warm*.sh scripts in the current directory
for script in ./warm*.sh; do
  echo "running $script"   
 ./"$script"
 echo "sleeping for 5seconds to reset-(ideally)"
 sleep 5 
done
