#!/usr/bin/bash

rm -r build 2> /dev/null || true
docker run --rm -v "$(pwd)":/work docker-registry2.spimageworks.com/spi/centos:7 bash -c '
set -e -x
yum install -y \
    libcap-devel \
    rsync \
    gcc \
    rpmdevtools \
    rpm-build \
    python-pip \
    python37

cat << EOF > /etc/pip.conf
[global]
trusted-host = pypi.spimageworks.com
index-url = http://pypi.spimageworks.com/spi/dev/
EOF

pip install pipenv

cd /work
rpmdev-setuptree
rpmbuild -ba spenv.spec
'
docker run --rm -v "$(pwd)":/work docker-registry2.spimageworks.com/spi/centos:7 chmod 777 -R /work/build

set -e -x
test -d build/rpm/x86_64
