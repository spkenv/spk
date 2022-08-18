Name: spk
Version: 0.36.0
Release: 1
Summary: Package manager and a software runtime for studio environments
License: NONE
URL: https://github.com/imageworks/spk
Source0: https://github.com/imageworks/spk/archive/refs/tags/v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: git
BuildRequires: gcc-c++
BuildRequires: libcap-devel
BuildRequires: openssl-devel
BuildRequires: python3-devel
BuildRequires: python3-pip
BuildRequires: cmake3
BuildRequires: make
Requires: bash

%define debug_package %{nil}

%description
Package manager and a software runtime for studio environments

%prep
%setup -q -n %{name}-%{version}

%build
cargo build --release --all --features=spfs/cli,spfs/server,spfs/protobuf-src

%install
mkdir -p %{buildroot}/usr/local/bin
RELEASE_DIR=%{_builddir}/%{name}-%{version}/target/release
for cmd in $RELEASE_DIR/spk $RELEASE_DIR/spfs $RELEASE_DIR/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/local/bin/
done
mv %{buildroot}/usr/local/bin/spk %{buildroot}/usr/local/bin/spk-%{version}

%files
/usr/local/bin/spfs
/usr/local/bin/spk-%{version}
%caps(cap_net_admin+ep) /usr/local/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/local/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/local/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/local/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf spk-%{version} /usr/local/bin/spk
