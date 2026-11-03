FROM almalinux:9

# several of the build tools below come from EPEL or CRB
RUN yum install -y epel-release dnf-plugins-core \
    && dnf config-manager --set-enabled crb

# --allowerasing swaps the image's coreutils-single for coreutils
RUN yum install -y --allowerasing \
    autoconf \
    autoconf-archive \
    automake \
    bison \
    coreutils \
    file \
    flex \
    gcc \
    gcc-c++ \
    gcc-toolset-14-binutils \
    gcc-toolset-14-gcc-c++ \
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
    unzip \
    wget \
    which \
    zip

RUN yum update -y

# the spk rpm includes spfs (and obsoletes the spfs rpm)
COPY rpm/RPMS/x86_64/spk-*.rpm /tmp/
RUN yum install -y /tmp/spk-*.rpm

RUN mkdir -p /spfs-storage /spfs-origin/{objects,payloads,tags}
COPY builder.spfs.toml /etc/spfs.toml
