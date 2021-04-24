#!/bin/bash

# To be executed in a docker container, depends heavily on the
# setup defined in .github/scripts/unittests.sh

export LANG=en_US.utf8

cd /source
yum install make rpm-build python36 deps/*.rpm -y > /dev/null 2>&1

(curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y) > /dev/null 2>&1
source /root/.cargo/env
# something gets poisoned in this shell script, where home is wrong for some reason in CI
sed -i 's|$HOME|/root|' /root/.bashrc

yum-builddep -y spk.spec > /dev/null 2>&1

pip3 install pipenv > /dev/null 2>&1
pipenv sync --dev > /dev/null 2>&1

sed -i "s|github.com|$SPFS_PULL_USERNAME:$SPFS_PULL_PASSWORD@github.com|" /source/Cargo.toml
make devel > /dev/null 2>&1
make test
