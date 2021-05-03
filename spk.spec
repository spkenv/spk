Name: spk
Version: 0.29.0
Release: 1
Summary: Package manager for SPFS.
License: NONE
URL: https://github.com/imageworks/spk
Source0: https://github.com/imageworks/spk/archive/refs/tags/v%{version}.tar.gz

BuildRequires: gcc
BuildRequires: git
BuildRequires: which
BuildRequires: gcc-c++
BuildRequires: chrpath
BuildRequires: libcap-devel
BuildRequires: openssl-devel
BuildRequires: python3-devel
BuildRequires: python3-pip
Requires: rsync
Requires: bash
Requires: spfs >= 0.28.0

%define debug_package %{nil}

%description
Package manager for SPFS

%prep
%setup -q -n %{name}-%{version}

%build
pip3 install pipenv
export LANG=en_US.UTF-8
mkdir -p ./build
pipenv sync --dev
source $(pipenv --venv)/bin/activate
python setup.py install
python -m nuitka \
    --standalone \
    --jobs $(nproc) \
    --follow-imports \
    --output-dir=./build \
    --include-package='sentry_sdk.integrations.stdlib' \
    --include-package='sentry_sdk.integrations.excepthook' \
    --include-package='sentry_sdk.integrations.dedupe' \
    --include-package='sentry_sdk.integrations.atexit' \
    --include-package='sentry_sdk.integrations.logging' \
    --include-package='sentry_sdk.integrations.argv' \
    --include-package='sentry_sdk.integrations.modules' \
    --include-package='sentry_sdk.integrations.threading' \
    spk

%install
mkdir -p %{buildroot}/usr/local/bin
mkdir -p %{buildroot}/opt/spk.dist
rsync -rvapog --chmod 755 %{_builddir}/%{name}-%{version}/build/spk.dist/* %{buildroot}/opt/spk.dist/

%files
/opt/spk.dist/

%preun
[ -e /usr/local/bin/spk ] && unlink /usr/local/bin/spk

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf /opt/spk.dist/spk /usr/local/bin/spk
