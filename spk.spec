Name: spk
Version: 0.31.0
Release: 1
Summary: Package manager for SPFS.
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
Requires: bash
Requires: spfs == 0.34.2

%define debug_package %{nil}

%description
Package manager for SPFS

%prep
%setup -q -n %{name}-%{version}

%build
cargo build --release

%install
mkdir -p %{buildroot}/usr/local/bin
install -m 0755 %{_builddir}/%{name}-%{version}/target/release/spk %{buildroot}/usr/local/bin/spk-%{version}

%files
/usr/local/bin/spk-%{version}

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf spk-%{version} /usr/local/bin/spk
