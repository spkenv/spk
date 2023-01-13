#!/bin/sh

# This file is for bootstrapping the environment where the integration tests
# can run successfully. It is intended to be run as root inside a container
# that has spfs installed, and the integration tests installed in /tests/.

set -o errexit

ORIGIN_REPO=/tmp/spfs-repos/origin
mkdir -p "$ORIGIN_REPO"
# Pre-create a repo
SPFS_REMOTE_origin_ADDRESS="file://${ORIGIN_REPO}?create=true" spfs ls-tags -r origin
export SPFS_REMOTE_origin_ADDRESS="file://${ORIGIN_REPO}"
# Run tests as a normal user to verify privilege escalation
useradd -m e2e
su e2e -c /tests/run_tests.sh