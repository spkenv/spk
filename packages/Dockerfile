from centos:7

RUN yum install -y \
    autoconf \
    autoconf-archive \
    autogen \
    automake \
    bison \
    coreutils \
    flex \
    gcc \
    gcc-c++ \
    gettext \
    gettext-devel \
    git \
    grep \
    help2man \
    libtool \
    m4 \
    make \
    patch \
    perl \
    sed \
    tar \
    texinfo \
    wget \
    which \
    zip

RUN yum update -y

COPY rpm/RPMS/x86_64/*.rpm /tmp/
RUN yum install -y /tmp/*.rpm

RUN mkdir -p /spfs-storage /spfs-origin/{objects,payloads,tags}
COPY builder.spfs.toml /etc/spfs.toml
