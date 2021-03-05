Name: spfs
Version: 0.22.1
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
mkdir -p ./build/bin
gcc -lcap -o ./build/bin/spfs-enter spfs-enter/main.c
dev toolchain install
source ~/.bashrc
dev env -- dev build spfs

%install
mkdir -p %{buildroot}/usr/bin
install -p -m 755 %{_builddir}/%{name}-v%{version}/build/spfs/release/spfs %{buildroot}/usr/bin/
install -p -m 755 %{_builddir}/%{name}-v%{version}/build/bin/spfs-enter %{buildroot}/usr/bin/

%files
/usr/bin/spfs
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin+ep) /usr/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf /usr/local/bin/spfs /usr/local/bin/spfs
