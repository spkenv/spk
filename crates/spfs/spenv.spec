Name: spenv
Version: 0.1.0
Release: 1
Summary: Runtime environment management.
License: NONE
Source0:  %{expand:%%(pwd)}

%description

%prep
find %{_sourcedir}/ -mindepth 1 -delete
rsync -rav \
    --delete \
    --cvs-exclude \
    --filter=":- .gitignore" \
    %{SOURCEURL0}/. %{_sourcedir}/

%build
cd %{_sourcedir}
./build.sh %{_builddir}/build

%install
mkdir -p %{buildroot}/usr/local/bin
install -p -m 755 %{_builddir}/build/bin/spenv %{buildroot}/usr/local/bin/
install -p -m 755 %{_builddir}/build/bin/spenv-mount %{buildroot}/usr/local/bin/
install -p -m 755 %{_builddir}/build/bin/spenv-remount %{buildroot}/usr/local/bin/

%files
/usr/local/bin/spenv
%caps(cap_setuid,cap_sys_admin+ep) /usr/local/bin/spenv-mount
%caps(cap_setuid,cap_sys_admin+ep) /usr/local/bin/spenv-remount
