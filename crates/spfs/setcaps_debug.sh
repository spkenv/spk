#!/usr/bin/bash
# Sets required capabilities on the local debug builds of spfs.
# Must be run as root, and ./target dir must be on a local filesystem (not NFS)

if [ "$EUID" -ne 0 ]
  then echo "Must be run as root, re-run with sudo"
  exit
fi

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

set -e
while read line; do
  echo "$line" | grep '%caps' > /dev/null || continue
  cmd=$(echo "$line" | sed -r 's|%caps\((.*)\) (.*)|setcap \1 \2|' | sed "s|/usr/bin/|$DIR/target/debug/|")
  echo $cmd
  $cmd
done < spfs.spec
