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

FROM build_env as rpm_build

ARG VERSION
ARG APP

COPY ${APP}.spec /root/rpmbuild/SPECS/
ENV VERSION ${VERSION}
ENV APP ${APP}
RUN echo "Building $APP @ $VERSION"

# ensure the current build version matches the one in the rpm
# spec file, or things can go awry
RUN test "$VERSION" == "$(cat /root/rpmbuild/SPECS/$APP.spec | grep Version | cut -d ' ' -f 2)"

RUN yum-builddep -y /root/rpmbuild/SPECS/$APP.spec && yum clean all

COPY . /source/$APP-$VERSION
RUN tar -C /source -czvf /root/rpmbuild/SOURCES/v$VERSION.tar.gz .

ENTRYPOINT ["sh", "-c", "rpmbuild -ba /root/rpmbuild/SPECS/$APP.spec"]
