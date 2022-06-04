Name: spfs
Version: 0.33.1
Release: 1
Summary: Filesystem isolation, capture, and distribution.
License: NONE
URL: https://gitlab.spimageworks.com/spi/dev/dev-ops/spfs
Source0: https://gitlab.spimageworks.com/spi/dev/dev-ops/spfs/-/archive/v%{version}/%{name}-v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: openssl-devel
# Minimum version with parallel component support and relocatable .spdev.yaml
BuildRequires: spdev >= 0.25.5

%define debug_package %{nil}

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q -n %{name}-v%{version}

%build
export SPDEV_CONFIG_FILE=.site/spi/.spdev.yaml
dev toolchain install
source ~/.bashrc
dev env -- cargo build --release --verbose --all --features sentry

%install
mkdir -p %{buildroot}/usr/bin
RELEASE_DIR=%{_builddir}/%{name}-v%{version}/target/release
for cmd in $RELEASE_DIR/spfs $RELEASE_DIR/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/bin/
done

%files
/usr/bin/spfs
/usr/bin/spfs-run
/usr/bin/spfs-shell
/usr/bin/spfs-push
/usr/bin/spfs-pull
/usr/bin/spfs-server
%caps(cap_net_admin+ep) /usr/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs
