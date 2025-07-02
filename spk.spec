Name: spk
Version: 0.44.0
Release: 1%{?dist}
Summary: Package manager and a software runtime for studio environments
License: NONE
URL: https://github.com/spkenv/spk
Source0: https://github.com/spkenv/spk/archive/refs/tags/v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: git
BuildRequires: gcc-c++
BuildRequires: libcap-devel
BuildRequires: openssl-devel
BuildRequires: python3-devel
BuildRequires: python3-pip
BuildRequires: fuse3-devel
# see explicit versions from dockerfile
# BuildRequires: flatbuffers-compiler
# BuildRequires: protobuf-compiler
BuildRequires: m4
BuildRequires: cmake3
BuildRequires: make
# not available in CentOS
# BuildRequires: flatbuffers-compiler
Requires: bash
Requires: fuse3
Requires: rsync
Obsoletes: spfs
Provides: spfs = 0.44.0

%define debug_package %{nil}

%description
Package manager and a software runtime for studio environments

%prep
%setup -q -n %{name}-%{version}

%build
cargo build --release --all --features=server,spfs/protobuf-src,fuse-backend-rhel-7-9

%install
mkdir -p %{buildroot}/usr/local/bin
RELEASE_DIR=%{_builddir}/%{name}-%{version}/target/release
for cmd in $RELEASE_DIR/spk $RELEASE_DIR/spfs $RELEASE_DIR/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    # skip windows
    if [[ $cmd =~ spfs-winfsp$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/local/bin/
done
mv %{buildroot}/usr/local/bin/spk %{buildroot}/usr/local/bin/spk-%{version}

%files
/usr/local/bin/spfs
/usr/local/bin/spk-%{version}
%caps(cap_dac_override,cap_fowner+ep) /usr/local/bin/spfs-clean
%caps(cap_net_admin+ep) /usr/local/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/local/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/local/bin/spfs-join
%caps(cap_dac_override,cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/local/bin/spfs-enter
%caps(cap_sys_admin+ep) /usr/local/bin/spfs-fuse

%post
mkdir -p /spfs
chmod 777 /spfs

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf spk-%{version} /usr/local/bin/spk
