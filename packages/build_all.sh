#!/usr/bin/bash

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
export SPK_BUILD_FLAGS="-dr origin -l"

cd $DIR
set -ex

bash ./bootstrap/build_all.sh

spk build $SPK_BUILD_FLAGS stdfs/stdfs.spk.yaml

bash ./gnu/build_all.sh

spk build $SPK_BUILD_FLAGS python/python2.spk.yaml
spk build $SPK_BUILD_FLAGS python/python3.spk.yaml
spk build $SPK_BUILD_FLAGS pip/pip.spk.yaml
