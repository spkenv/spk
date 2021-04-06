Name: spfs
Version: 0.26.0
Release: 1
Summary: Filesystem isolation, capture, and distribution.
License: NONE
URL: https://gitlab.spimageworks.com/dev-group/dev-ops/spfs
Source0: https://gitlab.spimageworks.com/dev-group/dev-ops/spfs/-/archive/v%{version}/%{name}-v%{version}.tar.gz

Requires: expect >= 5, expect < 6
BuildRequires: rsync
BuildRequires: gcc
BuildRequires: gcc-c++
BuildRequires: chrpath
BuildRequires: libcap-devel
BuildRequires: openssl-devel
BuildRequires: spdev

%define debug_package %{nil}

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q -n %{name}-v%{version}

%build
dev toolchain install
source ~/.bashrc
dev env -- dev build spfs

%install
mkdir -p %{buildroot}/usr/bin
for cmd in build/spfs/release/spfs build/spfs/release/spfs-*; do
    # skip debug info for commands
    if [[ $cmd =~ \.d$ ]]; then continue; fi
    install -p -m 755 %{_builddir}/%{name}-v%{version}/$cmd %{buildroot}/usr/bin/
done

%files
/usr/bin/spfs
/usr/bin/spfs-run
/usr/bin/spfs-shell
%caps(cap_sys_chroot,cap_sys_admin+ep) /usr/bin/spfs-join
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin+ep) /usr/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs
