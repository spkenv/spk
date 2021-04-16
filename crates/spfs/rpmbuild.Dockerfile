FROM centos:7
ARG VERSION

RUN yum install -y \
    curl \
    rpm-build \
    && yum clean all

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y
ENV PATH $PATH:/root/.cargo/bin

RUN mkdir -p /root/rpmbuild/{SOURCES,SPECS,RPMS,SRPMS}

COPY spfs.spec /root/rpmbuild/SPECS/
ENV VERSION ${VERSION}
RUN echo "Building for $VERSION"

# ensure the current build version matches the one in the rpm
# spec file, or things can go awry
RUN test "$VERSION" == "$(cat /root/rpmbuild/SPECS/spfs.spec | grep Version | cut -d ' ' -f 2)"

RUN yum-builddep -y /root/rpmbuild/SPECS/spfs.spec && yum clean all

COPY . /source/spfs-$VERSION
RUN tar -C /source -czvf /root/rpmbuild/SOURCES/v$VERSION.tar.gz .

RUN rpmbuild -ba /root/rpmbuild/SPECS/spfs.spec
