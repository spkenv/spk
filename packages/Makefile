# disables exporting packages in favour of touching the file, saves time and space
NOEXPORT ?= 0

.PHONY: packages
packages: \
	bootstrap

.PHONY: bootstrap bootstrap.phase1 bootstrap.phase2
bootstrap: bootstrap.phase1 bootstrap.phase2
bootstrap.phase1: \
	bootstrap/binutils.spk \
	bootstrap/gcc.spk \
	stdfs/stdfs.spk \
	bootstrap/glibc.spk \
	bootstrap/libstdcxx.spk
bootstrap.phase2: \
	bootstrap/m4.spk \
	bootstrap/ncurses.spk \
	bootstrap/bash.spk \
	bootstrap/coreutils.spk \
	bootstrap/diffutils.spk \
	bootstrap/file.spk \
	bootstrap/findutils.spk \
	bootstrap/gawk.spk \
	bootstrap/grep.spk \
	bootstrap/gzip.spk \
	bootstrap/make.spk \
	bootstrap/patch.spk \
	bootstrap/sed.spk \
	bootstrap/tar.spk \
	bootstrap/xz.spk \
	bootstrap/binutils.spk \
	bootstrap/gcc.spk

#####################################

.PHONY: gnu
gnu: \
	acl.spk \
	attr.spk \
	autoconf.spk \
	automake.spk \
	bash.spk \
	bc.spk \
	binutils.spk \
	bison.spk \
	bzip2.spk \
	check.spk \
	coreutils.spk \
	dbus.spk \
	dejagnu.spk \
	diffutils.spk \
	e2fsprogs.spk \
	elfutils.spk \
	eudev.spk \
	expat.spk \
	expect5.45.4 \
	file.spk \
	findutils.spk \
	flex.spk \
	gawk.spk \
	gcc.spk \
	gdbm.spk \
	gettext.spk \
	glibc.spk \
	gmp.spk \
	gperf.spk \
	grep.spk \
	groff.spk \
	grub.spk \
	gzip.spk \
	iana-etc.spk \
	inetutils.spk \
	intltool.spk \
	iproute2.spk \
	kbd.spk \
	kmod-28 \
	less.spk \
	lfs-bootscripts.spk \
	libcap.spk \
	libffi.spk \
	libpipeline.spk \
	libtool.spk \
	linux.spk \
	m4.spk \
	make.spk \
	man-db.spk \
	man-pages.spk \
	meson.spk \
	mpc.spk \
	mpfr.spk \
	ncurses.spk \
	ninja.spk \
	openssl-1.1.1j \
	patch.spk \
	perl.spk \
	pkg-config.spk \
	procps-ng.spk \
	psmisc.spk \
	Python.spk \
	python-3.9.2-docs-html2 \
	readline.spk \
	sed.spk \
	shadow.spk \
	sysklogd.spk \
	systemd.spk \
	systemd-man-pages.spk \
	sysvinit.spk \
	tar.spk \
	tcl8.6.11-src \
	tcl8.6.11-html \
	texinfo.spk \
	tzdata2021a \
	udev-lfs.spk \
	util-linux.spk \
	vim.spk \
	XML-Parser.spk \
	xz.spk \
	zlib.spk \
	zstd.spk \
	bzip2-1.0.8-install_docs-1.patch \
	coreutils-8.32-i18n-1.patch \
	glibc-2.33-fhs-1.patch \
	kbd-2.4.0-backspace-1.patch \
	sysvinit-2.98-consolidated-1.patch \
	systemd-247-upstream_fixes-1.patch \

perl: gnu perl/perl.spk

.PHONY: python python2 python3
python: python2 python3
python2: bootstrap bzip2 python/python2.spk
python3: bootstrap bzip2 python/python3.spk

.PHONY: clean
clean:
	find . -name "*.spk" -delete

.PHONY: import
import:
	find . -name "bootstrap" -prune -o -name "*.spk" -exec spk import {} \;

# bzip2: bzip2/bzip2.spk
%: $</$<.spk

%.spk : %.spk.yaml
	spk info -l $<@source > /dev/null 2>&1 || spk make-source $<
	SPFS_FILESYSTEM_TMPFS_SIZE=30G \
	spk mkb -dr origin -l -v $<
	if [ "$(NOEXPORT)" == "1" ]; then touch $@; else spk export $< $@; fi

docker.%:
	if [ ! -f ../dist/rpm/RPMS/x86_64/spk-*.rpm ]; then \
	echo "Please run 'make rpm' or download the latest spk rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	if [ ! -f ../dist/rpm/RPMS/x86_64/spfs-*.rpm ]; then \
	echo "Please build the spfs rpm or download the latest spfs rpm from github"; \
	echo "and place it into dist/rpm/RPMS/x86_64/ before continuing"; \
	exit 1; \
	fi
	docker build . -t spk-package-builder
	docker run --privileged --rm \
	-v $$PWD/../build/packages/:/spfs-storage \
	-v $$PWD/../dist/rpm/RPMS/x86_64:/rpms \
	-v $$PWD/..:/work spk-package-builder bash -ex -c "\
	yum install -y /rpms/* && \
	cd /work && \
	make packages.$*"
