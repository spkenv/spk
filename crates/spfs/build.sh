#!/usr/bin/bash

build_dir=${1:-build}

set -e -x

rm -r ${build_dir}/* || true
mkdir -p ${build_dir}/bin

gcc -lcap -o ${build_dir}/bin/spenv-enter spenv-enter/main.c

if [ "$(id -u)" == "0" ]; then
    setcap cap_setuid,cap_sys_admin+ep ${build_dir}/bin/spenv-enter
else
    echo "WARNING: not running as root, binary capabilities will not be set"
fi

pipenv install --dev
source "$(pipenv --venv)/bin/activate"
pipenv lock -r | grep -v -- "--trusted-host" > ${build_dir}/requirements.txt
echo "spenv==$(python setup.py --version)" >> ${build_dir}/requirements.txt
python setup.py clean
rm -r *.egg-info || true
python setup.py bdist_wheel
pex -c spenv -r ${build_dir}/requirements.txt . -o ${build_dir}/bin/spenv --disable-cache --repo=dist
