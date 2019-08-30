#!/usr/bin/bash

set -ex

rm -r build/* || true
mkdir -p build/bin

gcc -lcap -o build/bin/spenv-mount spenv-mount/main.c
sudo setcap cap_sys_admin+ep $(realpath build/bin/spenv-mount)

gcc -o build/bin/spenv-remount spenv-remount/main.c
sudo setcap cap_sys_admin+ep $(realpath build/bin/spenv-remount)

pipenv lock -r | grep -v -- "--trusted-host" > build/requirements.txt
python setup.py clean
rm -r *.egg-info || true
pex -m spenv -r build/requirements.txt . -o build/bin/spenv --disable-cache
