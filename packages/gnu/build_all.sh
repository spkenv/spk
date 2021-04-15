DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
export SPK_BUILD_FLAGS="-dr origin -l"

cd $DIR
set -ex

spk build $SPK_BUILD_FLAGS make.spk.yaml
spk build $SPK_BUILD_FLAGS autoconf.spk.yaml
spk build $SPK_BUILD_FLAGS gmp.spk.yaml
spk build $SPK_BUILD_FLAGS mpfr.spk.yaml
spk build $SPK_BUILD_FLAGS mpc.spk.yaml

spk build $SPK_BUILD_FLAGS gcc/gcc48.spk.yaml
spk build $SPK_BUILD_FLAGS gcc/gcc63.spk.yaml
spk build $SPK_BUILD_FLAGS gcc/gcc93.spk.yaml
