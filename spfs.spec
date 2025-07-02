Name: spfs
Version: 0.44.0
Release: 1%{?dist}
Summary: Filesystem isolation, capture, and distribution.
License: NONE
URL: https://github.com/spkenv/spk
Source0: https://github.com/spkenv/spk/archive/refs/tags/v%{version}.tar.gz


BuildRequires: gcc
BuildRequires: gcc-c++
BuildRequires: make
BuildRequires: cmake3
BuildRequires: openssl-devel
BuildRequires: fuse3-devel
# see explicit versions from dockerfile
# BuildRequires: flatbuffers-compiler
# BuildRequires: protobuf-compiler
BuildRequires: m4
Requires: fuse3
Requires: rsync

%define debug_package %{nil}

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q

%build
cargo build --release -p spfs -p spfs-cli-main -p spfs-cli-clean -p spfs-cli-enter -p spfs-cli-join -p spfs-cli-monitor -p spfs-cli-render --all --features=server,spfs/protobuf-src,fuse-backend-rhel-7-9

%install
mkdir -p %{buildroot}/usr/local/bin
RELEASE_DIR=%{_builddir}/%{name}-%{version}/target/release
for cmd in $RELEASE_DIR/spfs $RELEASE_DIR/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    # skip windows
    if [[ $cmd =~ spfs-winfsp$ ]]; then continue; fi
    install -p -m 755 $cmd %{buildroot}/usr/local/bin/
done

%files
/usr/local/bin/spfs
%caps(cap_dac_override,cap_fowner+ep) /usr/local/bin/spfs-clean
%caps(cap_net_admin+ep) /usr/local/bin/spfs-monitor
%caps(cap_chown,cap_fowner+ep) /usr/local/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/local/bin/spfs-join
%caps(cap_dac_override,cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/local/bin/spfs-enter
%caps(cap_sys_admin+ep) /usr/local/bin/spfs-fuse

%post
mkdir -p /spfs
chmod 777 /spfs
