#!/bin/bash

# To be executed in a docker container, depends heavily on the
# setup defined in .github/scripts/unittests.sh

export LANG=en_US.utf8

cd /source
yum install make rpm-build tcsh deps/*.rpm -y > /dev/null 2>&1

(curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y) > /dev/null 2>&1
source /root/.cargo/env
# something gets poisoned in this shell script, where home is wrong for some reason in CI
sed -i 's|$HOME|/root|' /root/.bashrc

yum-builddep -y spk.spec > /dev/null 2>&1

sed -i "s|github.com|$SPFS_PULL_USERNAME:$SPFS_PULL_PASSWORD@github.com|" /source/Cargo.toml

# there needs to be an origin configured even if it's not read from
# during testing (for commands that us the syncer type as a no-op)
export SPFS_REMOTE_origin_ADDRESS=file:///tmp/spfs-origin?create=true
make test
