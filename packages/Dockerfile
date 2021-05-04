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
    gperf \
    grep \
    help2man \
    libtool \
    m4 \
    make \
    perl \
    sed \
    texinfo \
    wget \
    which \
    zip

RUN mkdir -p /spfs-storage /spfs-origin/{objects,payloads,tags}
COPY builder.spfs.conf /etc/spfs.conf

