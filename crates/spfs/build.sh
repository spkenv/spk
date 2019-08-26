#!/usr/bin/bash

rm -r build
mkdir -p build/bin

gcc -o build/bin/spenv-mount spenv-mount/main.c
sudo setcap cap_sys_admin+ep build/bin/spenv-mount

gcc -o build/bin/spenv-remount spenv-remount/main.c
sudo setcap cap_sys_admin+ep build/bin/spenv-remount

pipenv lock -r > build/requirements.txt
python setup.py clean
rm -r *.egg-info
pex -m spenv -r build/requirements.txt . -o build/bin/spenv --disable-cache
