FROM docker-registry2.spimageworks.com/spi/centos:7

RUN yum install -y \
    libcap-devel \
    rsync \
    gcc \
    gcc-c++ \
    chrpath \
    rpmdevtools \
    rpm-build \
    python-pip \
    python37-devel

RUN echo '[global]' > /etc/pip.conf
RUN echo "trusted-host = pypi.spimageworks.com" >> /etc/pip.conf
RUN echo "index-url = http://pypi.spimageworks.com/spi/dev/" >> /etc/pip.conf

RUN pip install pipenv

WORKDIR /work
COPY . /work
RUN chown 0:0 -R /work

RUN rpmdev-setuptree
RUN rpmbuild -ba spenv.spec
