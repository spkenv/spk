Name: spfs
Version: 0.20.3
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
BuildRequires: python-pip
BuildRequires: python37-devel

%description
Filesystem isolation, capture, and distribution.

%prep
%setup -q -n %{name}-v%{version}

%build
mkdir -p ./build/bin
gcc -lcap -o ./build/bin/spfs-enter spfs-enter/main.c
pipenv sync --dev
source "$(pipenv --venv)/bin/activate"
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
    spfs

%install
mkdir -p %{buildroot}/usr/local/bin
mkdir -p %{buildroot}/opt/spfs.dist
rsync -rvapog --chmod 755 %{_builddir}/%{name}-v%{version}/build/spfs.dist/* %{buildroot}/opt/spfs.dist/
install -p -m 755 %{_builddir}/%{name}-v%{version}/build/bin/spfs-enter %{buildroot}/usr/local/bin/

%files
/opt/spfs.dist/
%caps(cap_setuid,cap_chown,cap_mknod,cap_sys_admin+ep) /usr/local/bin/spfs-enter

%post
mkdir -p /spfs
chmod 777 /spfs

%preun
[ -e /usr/local/bin/spfs ] && unlink /usr/local/bin/spfs

%posttrans
# must run at the absolute end in case we are updating
# and the uninstallation of the old version removes the symlink
ln -sf /opt/spfs.dist/spfs /usr/local/bin/spfs
