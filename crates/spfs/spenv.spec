Name: spenv
Version: %(grep -oP '(?<=_version__ = ").*(?=")' spenv/__init__.py)
Release: 1
Summary: Runtime environment management.
License: NONE
Source0:  %{expand:%%(pwd)}/

%description

%prep
find %{_sourcedir}/ -mindepth 1 -delete
rsync -rav \
    --delete \
    --cvs-exclude \
    --filter=":- .gitignore" \
    %{SOURCEURL0} %{_sourcedir}/

%build
build_dir="$(pwd)"
cd %{_sourcedir}
./build.sh "${build_dir}"

%install
mkdir -p %{buildroot}/usr/local/bin
mkdir -p %{buildroot}/opt
rsync -rvapog --chmod 755 %{_builddir}/spenv.dist/* %{buildroot}/opt/spenv.dist/
install -p -m 755 %{_builddir}/bin/spenv-enter %{buildroot}/usr/local/bin/

%files
/opt/spenv.dist/
%caps(cap_setuid,cap_sys_admin+ep) /usr/local/bin/spenv-enter

%post
ln -sf /opt/spenv.dist/spenv /usr/local/bin/spenv
mkdir -p /env
chmod 777 /env

%preun
unlink /usr/local/bin/spenv
