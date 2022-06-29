Name: spk
Version: 0.32.0
Release: 1
Summary: Package manager for SPFS.
License: NONE
URL: https://gitlab.spimageworks.com/spi/dev/dev-ops/spk
Source0: https://gitlab.spimageworks.com/spi/dev/dev-ops/spk/-/archive/v%{version}/%{name}-v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: git
BuildRequires: gcc-c++
BuildRequires: libcap-devel
BuildRequires: openssl-devel
# Minimum version with parallel component support and relocatable .spdev.yaml
BuildRequires: spdev >= 0.25.5

Requires: bash
Requires: spfs == 0.34.3

%define debug_package %{nil}

%description
Package manager for SPFS

%prep
%setup -q -n %{name}-v%{version}

%build
export SPDEV_CONFIG_FILE=.site/spi/.spdev.yaml
dev toolchain install
source ~/.bashrc
# Include `--all` to also build spk-launcher
dev env -- cargo build --release --features sentry --all

%install
mkdir -p %{buildroot}/usr/local/bin
cp %{_builddir}/%{name}-v%{version}/target/release/spk-launcher %{buildroot}/usr/local/bin/
mkdir -p %{buildroot}/opt/spk.dist
cp %{_builddir}/%{name}-v%{version}/target/release/spk %{buildroot}/opt/spk.dist/

%files
/usr/local/bin/spk-launcher
/opt/spk.dist/

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf spk-launcher /usr/local/bin/spk
