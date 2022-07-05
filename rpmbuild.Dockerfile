FROM centos:7
ARG VERSION
ARG SPFS_PULL_USERNAME
ARG SPFS_PULL_PASSWORD

RUN yum install -y \
    curl \
    rpm-build \
    && yum clean all

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y
ENV PATH $PATH:/root/.cargo/bin

RUN mkdir -p /root/rpmbuild/{SOURCES,SPECS,RPMS,SRPMS}

COPY spk.spec /root/rpmbuild/SPECS/
ENV VERSION ${VERSION}
RUN echo "Building for $VERSION"

# ensure the current build version matches the one in the rpm
# spec file, or things can go awry
RUN test "$VERSION" == "$(cat /root/rpmbuild/SPECS/spk.spec | grep Version | cut -d ' ' -f 2)"

RUN yum-builddep -y /root/rpmbuild/SPECS/spk.spec && yum clean all

COPY . /source/spk-$VERSION
ENV SPFS_PULL_USERNAME ${SPFS_PULL_USERNAME}
ENV SPFS_PULL_PASSWORD ${SPFS_PULL_PASSWORD}
RUN find /source -name "Cargo.toml" -exec sed -i "s|github.com|$SPFS_PULL_USERNAME:$SPFS_PULL_PASSWORD@github.com|" "{}" \;
RUN tar -C /source -czvf /root/rpmbuild/SOURCES/v$VERSION.tar.gz .

RUN rpmbuild -ba /root/rpmbuild/SPECS/spk.spec
