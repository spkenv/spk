Name: spfs
Version: 0.30.0
Release: 1
Summary: Filesystem isolation, capture, and distribution.
License: NONE
URL: https://github.com/imageworks/spfs
Source0: https://github.com/imageworks/spfs/archive/refs/tags/v%{version}.tar.gz


Requires: expect >= 5, expect < 6
BuildRequires: rsync
BuildRequires: gcc
BuildRequires: gcc-c++
BuildRequires: chrpath
BuildRequires: libcap-devel
BuildRequires: openssl-devel

%define debug_package %{nil}

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q

%build
cargo build --release --verbose

%install
mkdir -p %{buildroot}/usr/bin
RELEASE_DIR=%{_builddir}/%{name}-%{version}/target/release
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
/usr/bin/spfs-init
%caps(cap_chown,cap_fowner+ep) /usr/bin/spfs-render
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin,cap_fowner+ep) /usr/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs
