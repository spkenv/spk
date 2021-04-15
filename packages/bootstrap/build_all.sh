#!/usr/bin/bash

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
export SPK_BUILD_FLAGS="-dr origin -l"

cd $DIR
set -ex

spk build $SPK_BUILD_FLAGS gcc.spk.yaml
