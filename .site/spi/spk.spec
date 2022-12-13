Name: spk
Version: 0.36.0
Release: 1
Summary: Package manager and a software runtime for studio environments
License: NONE
URL: https://gitlab.spimageworks.com/spi/dev/dev-ops/spk
Source0: https://gitlab.spimageworks.com/spi/dev/dev-ops/spk/-/archive/v%{version}/%{name}-v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: git
BuildRequires: gcc-c++
BuildRequires: libcap-devel
BuildRequires: openssl-devel
BuildRequires: cmake3
BuildRequires: make
BuildRequires: spdev >= 0.28.2

Requires: bash
Obsoletes: spfs
Provides: spfs = 0.34.6

%define debug_package %{nil}

%description
Package manager and a software runtime for studio environments

%prep
%setup -q -n %{name}-v%{version}

%build
export SPDEV_CONFIG_FILE=.site/spi/.spdev.yaml
dev toolchain install
source ~/.bashrc
export SENTRY_DSN=http://4506b47108ac4b648fdf18a8d803f403@sentry.spimageworks.com/25
export SENTRY_ENVIRONMENT=production
# Include `--all` to also build spk-launcher
dev env -- cargo build --release --features "migration-to-components,runtime-compat-0.33,sentry,spfs/protobuf-src" --all

%install
mkdir -p %{buildroot}/usr/local/bin
RELEASE_DIR=%{_builddir}/%{name}-v%{version}/target/release
for cmd in "$RELEASE_DIR"/spk-launcher "$RELEASE_DIR"/spfs "$RELEASE_DIR"/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/local/bin/
done
mkdir -p %{buildroot}/opt/spk.dist
cp "$RELEASE_DIR"/spk %{buildroot}/opt/spk.dist/

%files
/usr/local/bin/spfs
%caps(cap_dac_override,cap_fowner+ep) /usr/local/bin/spfs-clean
%caps(cap_net_admin+ep) /usr/local/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/local/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/local/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/local/bin/spfs-enter
/usr/local/bin/spk-launcher
/opt/spk.dist/

%post
mkdir -p /spfs
chmod 777 /spfs

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf spk-launcher /usr/local/bin/spk
