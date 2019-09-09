#!/usr/bin/bash

build_dir=${1:-build}

set -e

rm -r ${build_dir}/* || true
mkdir -p ${build_dir}/bin

gcc -lcap -o ${build_dir}/bin/spenv-mount spenv-mount/main.c
gcc -o ${build_dir}/bin/spenv-remount spenv-remount/main.c

if [ "$(id -u)" == "0" ]; then
    setcap cap_setuid,cap_sys_admin+ep ${build_dir}/bin/spenv-mount
    setcap cap_setuid,cap_sys_admin+ep ${build_dir}/bin/spenv-remount
else
    echo "WARNING: not running as root, binary capabilities will not be set"
fi

pipenv lock -r | grep -v -- "--trusted-host" > ${build_dir}/requirements.txt
python setup.py clean
rm -r *.egg-info || true
pex -m spenv -r ${build_dir}/requirements.txt . -o ${build_dir}/bin/spenv --disable-cache
