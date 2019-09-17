#!/usr/bin/bash

set -e -x

image="localhost/spenv-builder:latest"
container="spenv-builder"
docker build -t $image .
docker create --name ${container} ${image}
rm -r build 2> /dev/null || true
docker cp ${container}:/root/rpmbuild/RPMS ./build
docker rm ${container}

test -d BUILD/x86_64
