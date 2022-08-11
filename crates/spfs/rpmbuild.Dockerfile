FROM centos:7 as build_env

RUN yum install -y \
    epel-release \
    curl \
    rpm-build \
    && yum clean all

RUN ln -s cmake3 /usr/bin/cmake
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y
ENV PATH $PATH:/root/.cargo/bin

RUN mkdir -p /root/rpmbuild/{SOURCES,SPECS,RPMS,SRPMS}

FROM build_env as spfs_build

ARG VERSION

COPY spfs.spec /root/rpmbuild/SPECS/
ENV VERSION ${VERSION}
RUN echo "Building for $VERSION"

# ensure the current build version matches the one in the rpm
# spec file, or things can go awry
RUN test "$VERSION" == "$(cat /root/rpmbuild/SPECS/spfs.spec | grep Version | cut -d ' ' -f 2)"

RUN yum-builddep -y /root/rpmbuild/SPECS/spfs.spec && yum clean all

COPY . /source/spfs-$VERSION
RUN tar -C /source -czvf /root/rpmbuild/SOURCES/v$VERSION.tar.gz .

ENTRYPOINT ["rpmbuild", "-ba", "/root/rpmbuild/SPECS/spfs.spec"]
