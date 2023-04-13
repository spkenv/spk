Name: spfs
Version: 0.34.6
Release: 1
Summary: Filesystem isolation, capture, and distribution.
License: NONE
URL: https://github.com/imageworks/spfs
Source0: https://github.com/imageworks/spfs/archive/refs/tags/v%{version}.tar.gz


BuildRequires: gcc
BuildRequires: gcc-c++
BuildRequires: make
BuildRequires: cmake3
BuildRequires: openssl-devel
BuildRequires: fuse-devel
BuildRequires: m4
Requires: fuse

%define debug_package %{nil}

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q

%build
cargo build --release -p spfs -p spfs-cli-main -p spfs-cli-clean -p spfs-cli-enter -p spfs-cli-join -p spfs-cli-monitor -p spfs-cli-render --verbose --all --features=server,spfs/protobuf-src,fuse-backend

%install
mkdir -p %{buildroot}/usr/local/bin
RELEASE_DIR=%{_builddir}/%{name}-%{version}/target/release
for cmd in $RELEASE_DIR/spfs $RELEASE_DIR/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/local/bin/
done

%files
/usr/local/bin/spfs
%caps(cap_dac_override,cap_fowner+ep) /usr/local/bin/spfs-clean
%caps(cap_net_admin+ep) /usr/local/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/local/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/local/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/local/bin/spfs-enter
%caps(cap_sys_admin+ep) /usr/local/bin/spfs-fuse

%post
mkdir -p /spfs
chmod 777 /spfs
