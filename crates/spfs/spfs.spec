Name: spfs
Version: 0.25.2
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
for cmd in "spfs spfs-run spfs-shell spfs-enter"; do
    install -p -m 755 %{_builddir}/%{name}-v%{version}/build/spfs/release/$cmd %{buildroot}/usr/bin/
done

%files
/usr/bin/spfs
/usr/bin/spfs-run
/usr/bin/spfs-shell
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin+ep) /usr/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs
